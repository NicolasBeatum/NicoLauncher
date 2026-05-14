use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use sha1::Digest as _;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tracing::{debug, warn};

use launcher_core::{Error, ProgressReporter, Result};

const MAX_RETRIES: u32 = 3;

#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub url: String,
    pub dest: PathBuf,
    /// SHA-1 expected hash (used by Mojang for libraries/assets)
    pub expected_sha1: Option<String>,
    /// SHA-512 expected hash (used for mods)
    pub expected_sha512: Option<String>,
    pub expected_size: Option<u64>,
    /// Fallback URLs tried in order if the primary URL returns 4xx
    pub fallback_urls: Vec<String>,
}

impl DownloadJob {
    pub fn new(url: impl Into<String>, dest: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            dest: dest.into(),
            expected_sha1: None,
            expected_sha512: None,
            expected_size: None,
            fallback_urls: Vec::new(),
        }
    }

    pub fn with_sha1(mut self, hash: impl Into<String>) -> Self {
        self.expected_sha1 = Some(hash.into());
        self
    }

    pub fn with_sha512(mut self, hash: impl Into<String>) -> Self {
        self.expected_sha512 = Some(hash.into());
        self
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.expected_size = Some(size);
        self
    }

    pub fn with_fallback_url(mut self, url: impl Into<String>) -> Self {
        self.fallback_urls.push(url.into());
        self
    }
}

pub struct Downloader {
    client: reqwest::Client,
    semaphore: Arc<Semaphore>,
    reporter: ProgressReporter,
}

impl Downloader {
    pub fn new(concurrency: usize, timeout_secs: u64, reporter: ProgressReporter) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent(concat!(
                "mc-launcher-template/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|e| Error::Other(e.to_string()))?;

        Ok(Self {
            client,
            semaphore: Arc::new(Semaphore::new(concurrency)),
            reporter,
        })
    }

    pub async fn download_many(&self, jobs: Vec<DownloadJob>) -> Result<()> {
        let total = jobs.len() as u64;
        self.reporter.stage("Downloading files", Some(total)).await;

        let results: Vec<Result<()>> = futures::stream::iter(jobs)
            .map(|job| {
                let client = self.client.clone();
                let sem = self.semaphore.clone();
                let reporter = self.reporter.clone();
                async move {
                    let _permit = sem.acquire().await.map_err(|e| Error::Other(e.to_string()))?;
                    let r = download_with_retry(&client, &job, &reporter).await;
                    reporter.advance(1).await;
                    r
                }
            })
            .buffer_unordered(128)
            .collect()
            .await;

        let errors: Vec<_> = results.into_iter().filter_map(|r| r.err()).collect();
        if errors.is_empty() {
            self.reporter.done().await;
            Ok(())
        } else {
            let msg = errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            Err(Error::Other(format!("{} download(s) failed: {msg}", errors.len())))
        }
    }

    pub async fn download_one(&self, job: DownloadJob) -> Result<()> {
        download_with_retry(&self.client, &job, &self.reporter).await
    }
}

async fn download_with_retry(client: &reqwest::Client, job: &DownloadJob, reporter: &launcher_core::ProgressReporter) -> Result<()> {
    let filename = job.dest
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?");

    // Fast path: skip if already present and valid
    if job.dest.exists() {
        if is_valid(job).await {
            debug!("Skip (cached): {:?}", job.dest);
            reporter.info(format!("✓ {filename}")).await;
            return Ok(());
        }
    }

    reporter.info(format!("⬇ {filename}")).await;

    // Build the list of URLs to try: primary first, then fallbacks
    let all_urls: Vec<&str> = std::iter::once(job.url.as_str())
        .chain(job.fallback_urls.iter().map(|s| s.as_str()))
        .collect();

    let mut last_err = None;
    'url_loop: for url in &all_urls {
        // Create a temporary job with this URL for download_once
        let mut attempt_job = job.clone();
        attempt_job.url = url.to_string();

        for attempt in 1..=MAX_RETRIES {
            match download_once(client, &attempt_job).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    let is_client_err = e.to_string().contains("HTTP 4");
                    warn!("Download attempt {attempt}/{MAX_RETRIES} failed for {url}: {e}");
                    last_err = Some(e);
                    // On 4xx, don't retry this URL — try the next fallback
                    if is_client_err { continue 'url_loop; }
                    if attempt < MAX_RETRIES {
                        tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
                    }
                }
            }
        }
    }
    Err(last_err.unwrap())
}

async fn download_once(client: &reqwest::Client, job: &DownloadJob) -> Result<()> {
    if let Some(parent) = job.dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let partial = job.dest.with_extension("partial");

    let resp = client
        .get(&job.url)
        .send()
        .await
        .map_err(|e| Error::Other(e.to_string()))?
        .error_for_status()
        .map_err(|e| Error::Other(format!("HTTP {} for {}: {e}", e.status().map_or(0, |s| s.as_u16()), job.url)))?;

    let mut file = tokio::fs::File::create(&partial).await?;

    // Hash while streaming so we don't need a second pass
    let mut sha1_hasher  = sha1::Sha1::new();
    let mut sha512_hasher = sha2::Sha512::new();
    let need_sha1   = job.expected_sha1.is_some();
    let need_sha512 = job.expected_sha512.is_some();

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::Other(e.to_string()))?;
        file.write_all(&chunk).await?;
        if need_sha1   { sha1_hasher.update(&chunk); }
        if need_sha512 { sha512_hasher.update(&chunk); }
    }
    file.flush().await?;
    drop(file);

    // Verify hashes before committing the file
    if let Some(expected) = &job.expected_sha1 {
        let actual = hex::encode(sha1_hasher.finalize());
        if actual != *expected {
            let _ = tokio::fs::remove_file(&partial).await;
            return Err(Error::HashMismatch {
                file: job.dest.clone(),
                expected: expected.clone(),
                actual,
            });
        }
    }
    if let Some(expected) = &job.expected_sha512 {
        let actual = hex::encode(sha512_hasher.finalize());
        if actual != *expected {
            let _ = tokio::fs::remove_file(&partial).await;
            return Err(Error::HashMismatch {
                file: job.dest.clone(),
                expected: expected.clone(),
                actual,
            });
        }
    }

    tokio::fs::rename(&partial, &job.dest).await?;
    Ok(())
}

/// Quick validity check.
/// Order: size check first (just metadata, fast), then hash only if size wrong/unknown.
async fn is_valid(job: &DownloadJob) -> bool {
    // Fast path: check file size first
    if let Some(expected_size) = job.expected_size {
        let actual_size = tokio::fs::metadata(&job.dest)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        if actual_size != expected_size {
            return false; // wrong size → re-download
        }
        // Size matches → trust it, skip hash check
        return true;
    }

    // No size available → fall back to hash verification
    if let Some(expected) = &job.expected_sha1 {
        return launcher_core::hash::verify_sha1(&job.dest, expected).await.is_ok();
    }
    if let Some(expected) = &job.expected_sha512 {
        return launcher_core::hash::verify_sha512(&job.dest, expected).await.is_ok();
    }

    // No criteria at all — assume valid
    true
}
