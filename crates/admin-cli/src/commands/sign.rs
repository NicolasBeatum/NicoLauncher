use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::{OsRng, RngCore};
use std::{collections::HashSet, path::Path};

// ═══════════════════════════════════════════════════════════════════════════════
// gen-keys
// ═══════════════════════════════════════════════════════════════════════════════

/// Genera un par de claves Ed25519 y las escribe como hex en signing.key / public.key.
pub async fn gen_keys(output_dir: Option<&Path>) -> Result<()> {
    let dir = output_dir.unwrap_or_else(|| Path::new("."));
    tokio::fs::create_dir_all(dir).await?;

    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    let signing_key = SigningKey::from_bytes(&seed);
    let private_hex = hex::encode(signing_key.to_bytes());
    let public_hex = hex::encode(signing_key.verifying_key().to_bytes());

    let signing_path = dir.join("signing.key");
    let public_path = dir.join("public.key");

    tokio::fs::write(&signing_path, &private_hex)
        .await
        .with_context(|| format!("No se pudo escribir {:?}", signing_path))?;
    tokio::fs::write(&public_path, &public_hex)
        .await
        .with_context(|| format!("No se pudo escribir {:?}", public_path))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&signing_path, std::fs::Permissions::from_mode(0o600))?;
    }

    println!("✅  Par de claves generado:");
    println!("   Clave privada → {:?}", signing_path);
    println!("   Clave pública → {:?}", public_path);
    println!();
    println!("⚠️   IMPORTANTE:");
    println!("   • Añade signing.key a .gitignore — NUNCA lo subas al repositorio.");
    println!("   • En CI, guarda el contenido de signing.key como secreto SIGNING_KEY.");
    println!("   • Copia el contenido de public.key en launcher.config.toml:");
    println!("     manifest_public_key = \"{}\"", public_hex);

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// sign
// ═══════════════════════════════════════════════════════════════════════════════

/// Firma un manifest.json con la clave privada y escribe <nombre>-signed.json.
pub async fn sign_manifest(
    manifest_path: &Path,
    key_path: &Path,
    output: Option<&Path>,
) -> Result<()> {
    let manifest_str = tokio::fs::read_to_string(manifest_path)
        .await
        .with_context(|| format!("Leyendo manifest {:?}", manifest_path))?;

    serde_json::from_str::<serde_json::Value>(&manifest_str)
        .context("El manifest no es JSON válido")?;

    let key_hex = tokio::fs::read_to_string(key_path)
        .await
        .with_context(|| format!("Leyendo clave privada {:?}", key_path))?;
    let key_hex = key_hex.trim();

    let key_bytes: Vec<u8> = hex::decode(key_hex).context("La clave privada no es hex válido")?;
    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("La clave privada debe ser exactamente 32 bytes"))?;
    let signing_key = SigningKey::from_bytes(&key_array);

    let signature = signing_key.sign(manifest_str.as_bytes());
    let sig_hex = hex::encode(signature.to_bytes());

    let signed = serde_json::json!({
        "manifest":  manifest_str,
        "signature": sig_hex,
    });

    let out_path = output.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        let stem = manifest_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        manifest_path.with_file_name(format!("{stem}-signed.json"))
    });

    tokio::fs::write(&out_path, serde_json::to_string_pretty(&signed)?)
        .await
        .with_context(|| format!("Escribiendo manifest firmado {:?}", out_path))?;

    println!("✅  Manifest firmado → {:?}", out_path);
    println!("   Firma Ed25519: {}…", &sig_hex[..16]);

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// verify
// ═══════════════════════════════════════════════════════════════════════════════

/// Verifica la firma Ed25519 de un manifest-signed.json contra una clave pública.
pub async fn verify_manifest(signed_path: &Path, key_path: &Path) -> Result<()> {
    use ed25519_dalek::Signature;

    let raw = tokio::fs::read_to_string(signed_path)
        .await
        .with_context(|| format!("Leyendo {:?}", signed_path))?;

    let value: serde_json::Value =
        serde_json::from_str(&raw).context("El archivo no es JSON válido")?;

    let manifest_str = value["manifest"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Campo 'manifest' no encontrado o no es string"))?;
    let sig_hex = value["signature"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Campo 'signature' no encontrado o no es string"))?;

    let key_hex = tokio::fs::read_to_string(key_path)
        .await
        .with_context(|| format!("Leyendo clave pública {:?}", key_path))?;
    let key_hex = key_hex.trim();

    let key_bytes: Vec<u8> = hex::decode(key_hex).context("La clave pública no es hex válido")?;
    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("La clave pública debe ser exactamente 32 bytes"))?;
    let verifying_key =
        VerifyingKey::from_bytes(&key_array).context("Clave pública Ed25519 inválida")?;

    let sig_bytes: Vec<u8> = hex::decode(sig_hex).context("La firma no es hex válido")?;
    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("La firma debe ser exactamente 64 bytes"))?;
    let signature = Signature::from_bytes(&sig_array);

    verifying_key
        .verify_strict(manifest_str.as_bytes(), &signature)
        .map_err(|e| anyhow::anyhow!("❌ Firma INVÁLIDA: {e}"))?;

    serde_json::from_str::<serde_json::Value>(manifest_str)
        .context("El manifest interno no es JSON válido")?;

    println!("✅  Firma válida. El manifest no ha sido alterado.");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// validate — validación completa
// ═══════════════════════════════════════════════════════════════════════════════

/// Informe de validación acumulado durante los checks.
#[derive(Default)]
struct Report {
    errors:   Vec<String>,
    warnings: Vec<String>,
}

impl Report {
    fn error(&mut self, msg: impl Into<String>) {
        self.errors.push(msg.into());
    }
    fn warn(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }
    #[allow(dead_code)]
    fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Valida un manifest.json (o manifest-signed.json) de forma exhaustiva.
///
/// Comprueba:
/// - Estructura y campos obligatorios
/// - Formato de hashes (sha512 = 128 chars hex)
/// - Duplicados de ID y filename
/// - Dependencias y conflictos de mods opcionales
/// - Paths seguros (sin rutas absolutas ni `..`)
/// - URLs no placeholder
/// - (con --check-urls) Accesibilidad HTTP real de todas las URLs
pub async fn validate_manifest(manifest_path: &Path, check_urls: bool) -> Result<()> {
    println!();
    println!("📋  Validando {}", manifest_path.display());

    // ── 1. Leer y extraer el JSON del manifest (firmado o no) ──────────────────
    let content = tokio::fs::read_to_string(manifest_path)
        .await
        .with_context(|| format!("Leyendo {:?}", manifest_path))?;

    let outer: serde_json::Value =
        serde_json::from_str(&content).context("El archivo no es JSON válido")?;

    let (manifest_str, is_signed) = if outer.get("signature").is_some() {
        let inner = outer["manifest"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Manifest firmado: campo 'manifest' no es string"))?;
        (inner.to_string(), true)
    } else {
        (content.clone(), false)
    };

    println!(
        "   Tipo: {}",
        if is_signed { "manifest firmado ✅" } else { "manifest sin firmar" }
    );

    let m: serde_json::Value =
        serde_json::from_str(&manifest_str).context("El manifest interno no es JSON válido")?;

    let mut report = Report::default();

    // ── 2. Estructura: campos obligatorios y valores ───────────────────────────
    println!();
    println!("── Estructura ─────────────────────────────────────────────────");

    check_field_u64(&m, "schema_version", &mut report);
    let mv = check_field_str(&m, "manifest_version", &mut report);
    let ra = check_field_str(&m, "released_at", &mut report);

    if let Some(v) = mv {
        println!("   ✅  manifest_version: {v}");
    }
    if let Some(v) = ra {
        // Validar que es ISO 8601
        if chrono::DateTime::parse_from_rfc3339(v).is_err() {
            report.error(format!("released_at no es ISO 8601 válido: {v:?}"));
            println!("   ❌  released_at: formato inválido ({v})");
        } else {
            println!("   ✅  released_at: {v}");
        }
    }

    // minecraft section
    match m.get("minecraft") {
        None => {
            report.error("Falta la sección minecraft");
            println!("   ❌  minecraft: no encontrado");
        }
        Some(mc) => {
            let mc_ver = mc["version"].as_str().unwrap_or("");
            let java_ver = mc["java_version"].as_u64();
            if mc_ver.is_empty() {
                report.error("Falta minecraft.version");
                println!("   ❌  minecraft.version: no encontrado");
            } else if !looks_like_mc_version(mc_ver) {
                report.warn(format!("minecraft.version parece inválido: {mc_ver:?}"));
                println!("   ⚠️   minecraft.version: {mc_ver} (formato inesperado)");
            } else {
                println!(
                    "   ✅  minecraft: {} (Java {})",
                    mc_ver,
                    java_ver
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "?".into())
                );
            }
        }
    }

    // loader section (opcional pero si existe, debe ser válida)
    if let Some(loader) = m.get("loader") {
        let ltype = loader["type"].as_str().unwrap_or("");
        let lver = loader["version"].as_str().unwrap_or("");
        let valid_types = ["neoforge", "forge", "fabric", "quilt", "vanilla"];
        if !valid_types.contains(&ltype) {
            report.error(format!("loader.type desconocido: {ltype:?}"));
            println!("   ❌  loader.type: {ltype:?} (esperado: neoforge/forge/fabric/quilt/vanilla)");
        } else if lver.is_empty() {
            report.warn("loader.version está vacío".to_string());
            println!("   ⚠️   loader: {ltype} (sin versión)");
        } else {
            println!("   ✅  loader: {ltype} {lver}");
        }
    }

    // ── 3. Mods requeridos ─────────────────────────────────────────────────────
    let req_mods = m["required_mods"].as_array().cloned().unwrap_or_default();

    if !req_mods.is_empty() {
        println!();
        println!("── Mods requeridos ({}) ───────────────────────────────────────", req_mods.len());

        let mut ids_seen: HashSet<String> = HashSet::new();
        let mut filenames_seen: HashSet<String> = HashSet::new();
        let mut modrinth_count = 0usize;
        let mut self_count = 0usize;
        let mut placeholder_count = 0usize;
        let mut mod_errors = 0usize;

        for (i, mod_v) in req_mods.iter().enumerate() {
            let id = mod_v["id"].as_str().unwrap_or("");
            let name = mod_v["name"].as_str().unwrap_or(id);
            let filename = mod_v["filename"].as_str().unwrap_or("");
            let sha512 = mod_v["sha512"].as_str().unwrap_or("");
            let size = mod_v["size"].as_u64().unwrap_or(0);

            // Campos requeridos
            if id.is_empty() {
                report.error(format!("required_mods[{i}]: falta 'id'"));
                mod_errors += 1;
            }
            if filename.is_empty() {
                report.error(format!("required_mods[{i}] ({name}): falta 'filename'"));
                mod_errors += 1;
            } else if !filename.ends_with(".jar") {
                report.warn(format!("required_mods[{i}] ({name}): filename no termina en .jar: {filename:?}"));
            }
            if !is_valid_sha512(sha512) {
                report.error(format!("required_mods[{i}] ({name}): sha512 inválido ({} chars)", sha512.len()));
                mod_errors += 1;
            }
            if size == 0 {
                report.warn(format!("required_mods[{i}] ({name}): size es 0"));
            }

            // Duplicados
            if !id.is_empty() && !ids_seen.insert(id.to_string()) {
                report.error(format!("required_mods: ID duplicado: {id:?}"));
                mod_errors += 1;
            }
            if !filename.is_empty() && !filenames_seen.insert(filename.to_string()) {
                report.error(format!("required_mods: filename duplicado: {filename:?}"));
                mod_errors += 1;
            }

            // Source
            if let Some(src) = mod_v.get("source") {
                if src.get("project_id").is_some() {
                    modrinth_count += 1;
                } else if let Some(url) = src["url"].as_str() {
                    self_count += 1;
                    if url.contains("TU_SERVIDOR") {
                        placeholder_count += 1;
                        report.warn(format!("{name} ({filename}): URL placeholder sin configurar"));
                    }
                } else {
                    report.error(format!("required_mods[{i}] ({name}): source sin url ni project_id"));
                    mod_errors += 1;
                }
            } else {
                report.error(format!("required_mods[{i}] ({name}): falta 'source'"));
                mod_errors += 1;
            }
        }

        if mod_errors == 0 {
            println!(
                "   ✅  {} Modrinth, {} self-hosted{}",
                modrinth_count,
                self_count,
                if self_count == 0 { String::new() } else { String::new() }
            );
        } else {
            println!("   ❌  {} error(es) en mods requeridos", mod_errors);
        }
        if ids_seen.len() == req_mods.len() {
            println!("   ✅  Sin IDs duplicados");
        }
        if filenames_seen.len() == req_mods.len() {
            println!("   ✅  Sin filenames duplicados");
        }
        if placeholder_count > 0 {
            println!("   ⚠️   {} mod(s) con URL placeholder (TU_SERVIDOR)", placeholder_count);
        }
    }

    // ── 4. Mods opcionales ─────────────────────────────────────────────────────
    let opt_mods = m["optional_mods"].as_array().cloned().unwrap_or_default();
    let opt_ids: HashSet<String> = opt_mods
        .iter()
        .filter_map(|m| m["id"].as_str().map(String::from))
        .collect();

    if !opt_mods.is_empty() {
        println!();
        println!("── Mods opcionales ({}) ───────────────────────────────────────", opt_mods.len());

        let mut opt_errors = 0usize;
        let mut dep_warnings = 0usize;
        let mut conflict_warnings = 0usize;

        // Mapa id → index para chequeos de simetría
        let opt_by_id: std::collections::HashMap<&str, &serde_json::Value> = opt_mods
            .iter()
            .filter_map(|m| m["id"].as_str().map(|id| (id, m)))
            .collect();

        for (i, mod_v) in opt_mods.iter().enumerate() {
            let id = mod_v["id"].as_str().unwrap_or("");
            let name = mod_v["name"].as_str().unwrap_or(id);
            let sha512 = mod_v["sha512"].as_str().unwrap_or("");

            // Campos básicos
            if id.is_empty() {
                report.error(format!("optional_mods[{i}]: falta 'id'"));
                opt_errors += 1;
            }
            if !is_valid_sha512(sha512) {
                report.error(format!("optional_mods[{i}] ({name}): sha512 inválido"));
                opt_errors += 1;
            }

            // Dependencias
            if let Some(deps) = mod_v["depends_on"].as_array() {
                let missing: Vec<&str> = deps
                    .iter()
                    .filter_map(|d| d.as_str())
                    .filter(|dep_id| !opt_ids.contains(*dep_id))
                    .collect();
                if !missing.is_empty() {
                    for dep in &missing {
                        report.warn(format!(
                            "{name}: depends_on \"{dep}\" no existe en optional_mods"
                        ));
                        dep_warnings += 1;
                    }
                }
            }

            // Conflictos
            if let Some(conflicts) = mod_v["conflicts_with"].as_array() {
                for conflict_v in conflicts {
                    let conflict_id = conflict_v.as_str().unwrap_or("");
                    if conflict_id.is_empty() { continue; }

                    if !opt_ids.contains(conflict_id) {
                        report.warn(format!(
                            "{name}: conflicts_with \"{conflict_id}\" no existe en optional_mods"
                        ));
                        conflict_warnings += 1;
                        continue;
                    }

                    // Verificar simetría: si A conflicta con B, B debe conflictar con A
                    let b_conflicts_a = opt_by_id.get(conflict_id)
                        .and_then(|b| b["conflicts_with"].as_array())
                        .map(|arr| arr.iter().any(|v| v.as_str() == Some(id)))
                        .unwrap_or(false);

                    if !b_conflicts_a {
                        report.warn(format!(
                            "Conflicto asimétrico: \"{id}\" ↔ \"{conflict_id}\" \
                             (\"{conflict_id}\" no declara conflicto con \"{id}\")"
                        ));
                        conflict_warnings += 1;
                    }
                }
            }

            // Auto-dependencia / auto-conflicto
            if mod_v["depends_on"].as_array()
                .map(|a| a.iter().any(|v| v.as_str() == Some(id)))
                .unwrap_or(false)
            {
                report.error(format!("{name}: depends_on se menciona a sí mismo"));
                opt_errors += 1;
            }
            if mod_v["conflicts_with"].as_array()
                .map(|a| a.iter().any(|v| v.as_str() == Some(id)))
                .unwrap_or(false)
            {
                report.error(format!("{name}: conflicts_with se menciona a sí mismo"));
                opt_errors += 1;
            }
        }

        if opt_errors == 0 && dep_warnings == 0 && conflict_warnings == 0 {
            println!("   ✅  Todos los mods opcionales son válidos");
            println!("   ✅  Dependencias y conflictos coherentes");
        } else {
            if opt_errors > 0 {
                println!("   ❌  {} error(es) en mods opcionales", opt_errors);
            }
            if dep_warnings > 0 {
                println!("   ⚠️   {} dependencia(s) con ID inexistente", dep_warnings);
            }
            if conflict_warnings > 0 {
                println!("   ⚠️   {} conflicto(s) con problema (ver advertencias)", conflict_warnings);
            }
        }
    }

    // ── 5. Config overrides ────────────────────────────────────────────────────
    let config_overrides = m["config_overrides"].as_array().cloned().unwrap_or_default();

    if !config_overrides.is_empty() {
        println!();
        println!("── Config overrides ({}) ──────────────────────────────────────", config_overrides.len());

        let mut cfg_errors = 0usize;
        let mut cfg_placeholders = 0usize;

        for (i, cfg) in config_overrides.iter().enumerate() {
            let path = cfg["path"].as_str().unwrap_or("");
            let sha512 = cfg["sha512"].as_str().unwrap_or("");
            let apply = cfg["apply"].as_str().unwrap_or("");
            let url = cfg["url"].as_str().unwrap_or("");

            if path.is_empty() {
                report.error(format!("config_overrides[{i}]: falta 'path'"));
                cfg_errors += 1;
            } else {
                if std::path::Path::new(path).is_absolute() {
                    report.error(format!("config_overrides[{i}]: path absoluto: {path:?}"));
                    cfg_errors += 1;
                }
                if path.contains("..") {
                    report.error(format!("config_overrides[{i}]: path con '..': {path:?}"));
                    cfg_errors += 1;
                }
            }

            if !is_valid_sha512(sha512) {
                report.error(format!("config_overrides[{i}] ({path}): sha512 inválido"));
                cfg_errors += 1;
            }

            if apply != "always" && apply != "if_missing" {
                report.error(format!(
                    "config_overrides[{i}] ({path}): apply debe ser \"always\" o \"if_missing\", no {apply:?}"
                ));
                cfg_errors += 1;
            }

            if url.contains("TU_SERVIDOR") {
                report.warn(format!("config_overrides[{i}] ({path}): URL placeholder"));
                cfg_placeholders += 1;
            }
        }

        if cfg_errors == 0 {
            println!(
                "   ✅  {} override(s) correctos{}",
                config_overrides.len(),
                if cfg_placeholders > 0 {
                    format!(" ({} placeholder(s) ⚠️)", cfg_placeholders)
                } else {
                    String::new()
                }
            );
        } else {
            println!("   ❌  {} error(es) en config_overrides", cfg_errors);
        }
    }

    // ── 6. removed_files ──────────────────────────────────────────────────────
    if let Some(removed) = m["removed_files"].as_array() {
        for (i, f) in removed.iter().enumerate() {
            if let Some(path) = f.as_str() {
                if std::path::Path::new(path).is_absolute() {
                    report.error(format!("removed_files[{i}]: path absoluto: {path:?}"));
                }
                if path.contains("..") {
                    report.error(format!("removed_files[{i}]: path con '..': {path:?}"));
                }
            }
        }
    }

    // ── 7. Comprobación de URLs (opcional) ─────────────────────────────────────
    if check_urls {
        let mut all_urls: Vec<(&str, String)> = Vec::new(); // (label, url)

        for mod_v in &req_mods {
            if let Some(src) = mod_v.get("source") {
                let name = mod_v["name"].as_str().unwrap_or("?");
                if let Some(url) = src["url"].as_str() {
                    all_urls.push((name, url.to_string()));
                } else if let Some(url) = src["download_url"].as_str() {
                    all_urls.push((name, url.to_string()));
                }
            }
        }
        for mod_v in &opt_mods {
            if let Some(src) = mod_v.get("source") {
                let name = mod_v["name"].as_str().unwrap_or("?");
                if let Some(url) = src["url"].as_str() {
                    all_urls.push((name, url.to_string()));
                } else if let Some(url) = src["download_url"].as_str() {
                    all_urls.push((name, url.to_string()));
                }
            }
        }
        for cfg in &config_overrides {
            if let Some(url) = cfg["url"].as_str() {
                let path = cfg["path"].as_str().unwrap_or("config");
                all_urls.push((path, url.to_string()));
            }
        }

        // Filtrar placeholders
        let checkable: Vec<_> = all_urls
            .iter()
            .filter(|(_, url)| !url.contains("TU_SERVIDOR") && url.starts_with("http"))
            .collect();

        if checkable.is_empty() {
            println!();
            println!("── URLs ───────────────────────────────────────────────────────");
            println!("   ⚠️   Sin URLs comprobables (todas son placeholders o vacías)");
        } else {
            println!();
            println!(
                "── URLs ({} a comprobar) ───────────────────────────────────────",
                checkable.len()
            );

            let http = reqwest::Client::builder()
                .user_agent("mc-launcher-template/validator")
                .timeout(std::time::Duration::from_secs(10))
                .build()?;

            let mut ok = 0usize;
            let mut fail = 0usize;

            for (label, url) in &checkable {
                match http.head(url.as_str()).send().await {
                    Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 403 => {
                        // 403 = existe pero requiere auth (Modrinth CDN a veces)
                        ok += 1;
                    }
                    Ok(resp) => {
                        let status = resp.status();
                        report.error(format!("{label}: HTTP {status} → {url}"));
                        println!("   ❌  {} → {} ({})", label, status, short_url(url));
                        fail += 1;
                    }
                    Err(e) if e.is_timeout() => {
                        report.warn(format!("{label}: timeout → {url}"));
                        println!("   ⚠️   {} → timeout ({})", label, short_url(url));
                        fail += 1;
                    }
                    Err(e) => {
                        report.warn(format!("{label}: error de red — {e}"));
                        println!("   ⚠️   {} → error: {e}", label);
                        fail += 1;
                    }
                }
            }

            if fail == 0 {
                println!("   ✅  {} URLs accesibles", ok);
            } else {
                println!("   {} accesibles, {} con problemas", ok, fail);
            }
        }
    }

    // ── 8. Resumen final ───────────────────────────────────────────────────────
    println!();
    println!("── Resumen ────────────────────────────────────────────────────");

    // Mostrar todas las advertencias y errores acumulados
    for w in &report.warnings {
        println!("   ⚠️   {w}");
    }
    for e in &report.errors {
        println!("   ❌  {e}");
    }

    if report.errors.is_empty() && report.warnings.is_empty() {
        println!("   Sin problemas detectados.");
    }

    println!();
    if report.errors.is_empty() {
        if report.warnings.is_empty() {
            println!("✅  Manifest válido.");
        } else {
            println!(
                "✅  Manifest válido con {} advertencia(s).",
                report.warnings.len()
            );
        }
    } else {
        println!(
            "❌  Manifest inválido — {} error(es), {} advertencia(s).",
            report.errors.len(),
            report.warnings.len()
        );
        bail!("La validación falló");
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn check_field_str<'a>(
    m: &'a serde_json::Value,
    field: &str,
    report: &mut Report,
) -> Option<&'a str> {
    match m.get(field).and_then(|v| v.as_str()) {
        Some(v) => Some(v),
        None => {
            report.error(format!("Falta campo obligatorio: {field:?}"));
            None
        }
    }
}

fn check_field_u64(m: &serde_json::Value, field: &str, report: &mut Report) -> Option<u64> {
    match m.get(field).and_then(|v| v.as_u64()) {
        Some(v) => Some(v),
        None => {
            report.error(format!("Falta campo obligatorio: {field:?}"));
            None
        }
    }
}

/// Un sha512 válido es exactamente 128 caracteres hexadecimales.
fn is_valid_sha512(s: &str) -> bool {
    s.len() == 128 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Versión de Minecraft "parece válida" si es N.N o N.N.N (1–3 dígitos cada parte).
fn looks_like_mc_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    matches!(parts.len(), 2..=3) && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn short_url(url: &str) -> String {
    if url.len() <= 60 {
        url.to_string()
    } else {
        format!("{}…", &url[..57])
    }
}
