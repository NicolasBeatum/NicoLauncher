use std::time::Duration;

use base64::Engine as _;
use rand::Rng as _;
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{debug, info};

use launcher_core::{Error, Result};

// Must use "consumers" — XboxLive.signin scope only exists in the personal-accounts tenant.
const AUTH_ENDPOINT: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
const TOKEN_ENDPOINT: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const SCOPES: &str = "XboxLive.signin offline_access";
const OAUTH_TIMEOUT_SECS: u64 = 300;

/// Fixed local port for the OAuth callback server.
/// Register EXACTLY this URI in Azure Portal:
///   http://localhost:25558/callback
/// Platform: "Mobile and desktop applications"
pub const REDIRECT_URI: &str = "http://localhost:25558/callback";
const REDIRECT_PORT: u16 = 25558;

#[derive(Debug)]
pub struct MsTokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Seconds until access_token expires
    pub expires_in: u64,
}

/// Full PKCE OAuth flow: open browser → wait for callback → exchange code → return tokens.
pub async fn login(client_id: &str, http: &reqwest::Client) -> Result<MsTokens> {
    let code_verifier  = generate_code_verifier();
    let code_challenge = compute_code_challenge(&code_verifier);

    // Use fixed port so the redirect_uri is always predictable.
    // Azure registration must have EXACTLY: http://localhost:25558/callback
    let listener = TcpListener::bind(format!("127.0.0.1:{REDIRECT_PORT}"))
        .await
        .map_err(|e| Error::Auth(format!(
            "Cannot start OAuth callback server on port {REDIRECT_PORT}: {e}\n\
             Is another process using that port?"
        )))?;

    let auth_url = format!(
        "{AUTH_ENDPOINT}?client_id={client_id}&response_type=code\
         &redirect_uri={}&scope={}&code_challenge={code_challenge}\
         &code_challenge_method=S256&prompt=select_account",
        urlencoded(REDIRECT_URI),
        urlencoded(SCOPES),
    );

    info!("Opening browser for Microsoft login…");
    println!("\n  Opening your browser for Microsoft login.");
    println!("  If nothing opens, visit:\n  {auth_url}\n");

    let url_for_browser = auth_url.clone();
    let _ = tokio::task::spawn_blocking(move || open::that(&url_for_browser)).await;

    info!("Waiting for OAuth callback on port {REDIRECT_PORT} (timeout {}s)…", OAUTH_TIMEOUT_SECS);
    let code = tokio::time::timeout(
        Duration::from_secs(OAUTH_TIMEOUT_SECS),
        wait_for_callback(listener),
    )
    .await
    .map_err(|_| Error::Auth("Login timed out (5 minutes). Please try again.".into()))??;

    debug!("Got auth code, exchanging for tokens…");
    exchange_code(http, client_id, &code, REDIRECT_URI, &code_verifier).await
}

/// Refresh an existing access token using the stored refresh token.
pub async fn refresh(
    client_id: &str,
    refresh_token: &str,
    http: &reqwest::Client,
) -> Result<MsTokens> {
    let params = [
        ("grant_type",    "refresh_token"),
        ("client_id",     client_id),
        ("refresh_token", refresh_token),
        ("scope",         SCOPES),
    ];

    let resp: serde_json::Value = http
        .post(TOKEN_ENDPOINT)
        .form(&params)
        .send()
        .await
        .map_err(|e| Error::Auth(e.to_string()))?
        .json()
        .await
        .map_err(|e| Error::Auth(e.to_string()))?;

    parse_token_response(resp)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

async fn wait_for_callback(listener: TcpListener) -> Result<String> {
    let (mut stream, _) = listener
        .accept()
        .await
        .map_err(|e| Error::Auth(format!("OAuth callback error: {e}")))?;

    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| Error::Auth(e.to_string()))?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Respond with a friendly page so the browser tab can close
    let body = b"<html><body><h2>Login successful!</h2><p>You can close this tab.</p></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.write_all(body).await;

    // Parse the first line: GET /callback?code=xxx&...
    let first_line = request.lines().next().unwrap_or("");
    let query = first_line
        .split_whitespace()
        .nth(1)
        .and_then(|path| path.split_once('?'))
        .map(|(_, q)| q)
        .unwrap_or("");

    let mut code = None;
    let mut error = None;
    let mut error_description = None;

    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            match key {
                "code"              => code              = Some(urldecoded(value)),
                "error"             => error             = Some(urldecoded(value)),
                "error_description" => error_description = Some(urldecoded(value)),
                _ => {}
            }
        }
    }

    if let Some(code) = code {
        return Ok(code);
    }

    if let Some(err) = error {
        let desc = error_description.unwrap_or_default();
        return Err(Error::Auth(format!("Microsoft OAuth error: {err} — {desc}")));
    }

    Err(Error::Auth(
        "OAuth callback did not contain a code. Did you cancel the login?".into(),
    ))
}

async fn exchange_code(
    http: &reqwest::Client,
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<MsTokens> {
    let params = [
        ("grant_type",    "authorization_code"),
        ("client_id",     client_id),
        ("code",          code),
        ("redirect_uri",  redirect_uri),
        ("code_verifier", code_verifier),
    ];

    let resp: serde_json::Value = http
        .post(TOKEN_ENDPOINT)
        .form(&params)
        .send()
        .await
        .map_err(|e| Error::Auth(e.to_string()))?
        .json()
        .await
        .map_err(|e| Error::Auth(e.to_string()))?;

    parse_token_response(resp)
}

fn parse_token_response(resp: serde_json::Value) -> Result<MsTokens> {
    if let Some(err) = resp.get("error") {
        let desc = resp
            .get("error_description")
            .and_then(|d| d.as_str())
            .unwrap_or("");
        return Err(Error::Auth(format!("MS token error: {err} — {desc}")));
    }

    Ok(MsTokens {
        access_token: json_str(&resp, "access_token")?,
        refresh_token: json_str(&resp, "refresh_token")?,
        expires_in: resp
            .get("expires_in")
            .and_then(|v| v.as_u64())
            .unwrap_or(3600),
    })
}

// ── PKCE helpers ──────────────────────────────────────────────────────────────

fn generate_code_verifier() -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut rng = rand::rng();
    (0..128)
        .map(|_| CHARS[rng.random_range(0..CHARS.len())] as char)
        .collect()
}

fn compute_code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash)
}

// ── URL helpers ───────────────────────────────────────────────────────────────

fn urlencoded(s: &str) -> String {
    s.bytes().fold(String::new(), |mut acc, b| {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                acc.push(b as char);
            }
            _ => acc.push_str(&format!("%{b:02X}")),
        }
        acc
    })
}

fn urldecoded(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next().unwrap_or('0');
            let h2 = chars.next().unwrap_or('0');
            if let Ok(b) = u8::from_str_radix(&format!("{h1}{h2}"), 16) {
                result.push(b as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

pub fn json_str(v: &serde_json::Value, key: &str) -> Result<String> {
    v.get(key)
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| Error::Auth(format!("Missing field '{key}' in response: {v}")))
}
