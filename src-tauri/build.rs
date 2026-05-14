use std::fs;
use std::path::PathBuf;

fn main() {
    tauri_build::build();

    let config_path = "../launcher.config.toml";
    println!("cargo:rerun-if-changed={config_path}");

    let content = fs::read_to_string(config_path)
        .expect("Cannot read launcher.config.toml — make sure it exists in the repo root");
    let config: toml::Value = toml::from_str(&content)
        .expect("Cannot parse launcher.config.toml");

    let b = &config["branding"];
    let s = &config["server"];

    let display_name   = b["display_name"].as_str().unwrap_or("MC Launcher");
    let window_title   = b["window_title"].as_str().unwrap_or("MC Launcher");
    let primary_color  = b["primary_color"].as_str().unwrap_or("#7c3aed");
    let secondary_color = b["secondary_color"].as_str().unwrap_or("#1e293b");
    let accent_color   = b["accent_color"].as_str().unwrap_or("#f59e0b");
    let heading_font   = b["heading_font"].as_str().unwrap_or("Inter");
    let body_font      = b["body_font"].as_str().unwrap_or("Inter");
    let internal_id    = b["internal_id"].as_str().unwrap_or("mc-launcher-template");

    let discord = b.get("social").and_then(|s| s.get("discord")).and_then(|v| v.as_str()).unwrap_or("");
    let website = b.get("social").and_then(|s| s.get("website")).and_then(|v| v.as_str()).unwrap_or("");

    let server_name    = s["display_name"].as_str().unwrap_or("Mi Servidor");
    let server_address = s["address"].as_str().unwrap_or("");
    let server_port    = s["port"].as_integer().unwrap_or(25565) as u16;

    // Write branding.json for embedding in the binary (served via get_branding command)
    let branding = serde_json::json!({
        "internalId":     internal_id,
        "displayName":    display_name,
        "windowTitle":    window_title,
        "primaryColor":   primary_color,
        "secondaryColor": secondary_color,
        "accentColor":    accent_color,
        "headingFont":    heading_font,
        "bodyFont":       body_font,
        "discord":        discord,
        "website":        website,
        "serverName":     server_name,
        "serverAddress":  server_address,
        "serverPort":     server_port,
    });

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("branding.json"), branding.to_string())
        .expect("Cannot write branding.json");
}
