//! Pheromone binary update manager.
//!
//! Queries burrow for the latest pheromone versions, downloads updated
//! tarballs via burrow (which handles caching and dev-mode), extracts
//! the binary into a global cache, and symlinks it into the per-project
//! pheromone directory.
//!
//! Global cache layout:  ~/.wezel/pheromones/{name}/{version}/{name}
//! Per-project layout:   {pheromone_dir}/{name}  →  symlink to global cache

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Minimal subset of the `/api/pheromones` response we need.
#[derive(Debug, Deserialize)]
struct PheromoneEntry {
    name: String,
    version: String,
    platforms: Vec<String>,
}

/// Returns the target triple for the current platform, or `None` if unknown.
fn current_target() -> Option<&'static str> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return Some("aarch64-apple-darwin");
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return Some("x86_64-apple-darwin");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return Some("x86_64-unknown-linux-gnu");
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return Some("aarch64-unknown-linux-gnu");
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return Some("x86_64-pc-windows-msvc");
    #[allow(unreachable_code)]
    None
}

fn global_cache_dir() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".wezel")
        .join("pheromones")
}

/// Path to the extracted binary in the global cache.
fn cached_binary_path(name: &str, version: &str) -> PathBuf {
    global_cache_dir().join(name).join(version).join(name)
}

fn extract_binary(tarball: &[u8], binary_name: &str, dest: &Path) -> anyhow::Result<()> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let gz = GzDecoder::new(tarball);
    let mut archive = Archive::new(gz);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if file_name == binary_name {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            // Write atomically via temp file.
            let tmp = dest.with_extension("tmp");
            std::fs::write(&tmp, &bytes)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&tmp)?.permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&tmp, perms)?;
            }
            std::fs::rename(&tmp, dest)?;
            return Ok(());
        }
    }
    anyhow::bail!("binary '{binary_name}' not found in tarball")
}

fn ensure_symlink(target: &Path, link: &Path) -> anyhow::Result<()> {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Remove stale link or old binary.
    let _ = std::fs::remove_file(link);
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link)?;
    #[cfg(not(unix))]
    std::fs::copy(target, link).map(|_| ())?;
    Ok(())
}

/// Check burrow for updated pheromone versions and install any that are newer.
///
/// * `server_url`       — base URL of burrow (e.g. `http://localhost:3001`)
/// * `pheromone_dir`    — per-project directory where symlinks are placed
pub fn update_pheromones(server_url: &str, pheromone_dir: &Path) {
    let Some(target) = current_target() else {
        log::debug!("pheromone_mgr: unknown platform, skipping update");
        return;
    };

    let url = format!("{}/api/pheromones", server_url.trim_end_matches('/'));
    let agent = ureq::Agent::new();
    let entries: Vec<PheromoneEntry> = match agent.get(&url).call() {
        Ok(r) => match r.into_json() {
            Ok(v) => v,
            Err(e) => {
                log::warn!("pheromone_mgr: failed to parse pheromones: {e}");
                return;
            }
        },
        Err(e) => {
            log::warn!("pheromone_mgr: failed to fetch pheromones: {e}");
            return;
        }
    };

    for entry in &entries {
        if !entry.platforms.iter().any(|p| p == target) {
            log::debug!(
                "pheromone_mgr: {} not available for {target}, skipping",
                entry.name
            );
            continue;
        }

        let cached = cached_binary_path(&entry.name, &entry.version);
        let link = pheromone_dir.join(&entry.name);

        if !cached.exists() {
            log::info!(
                "pheromone_mgr: downloading {} {}",
                entry.name,
                entry.version
            );
            let binary_url = format!(
                "{}/api/pheromone/{}/binary/{target}",
                server_url.trim_end_matches('/'),
                entry.name,
            );
            let tarball = match agent.get(&binary_url).call() {
                Ok(r) => {
                    let mut buf = Vec::new();
                    if let Err(e) = r.into_reader().read_to_end(&mut buf) {
                        log::warn!(
                            "pheromone_mgr: failed to read tarball for {}: {e}",
                            entry.name
                        );
                        continue;
                    }
                    buf
                }
                Err(e) => {
                    log::warn!("pheromone_mgr: failed to download {}: {e}", entry.name);
                    continue;
                }
            };

            if let Err(e) = extract_binary(&tarball, &entry.name, &cached) {
                log::warn!("pheromone_mgr: failed to extract {}: {e}", entry.name);
                continue;
            }
            log::info!("pheromone_mgr: installed {} {}", entry.name, entry.version);
        } else {
            log::debug!(
                "pheromone_mgr: {} {} already cached",
                entry.name,
                entry.version
            );
        }

        if let Err(e) = ensure_symlink(&cached, &link) {
            log::warn!("pheromone_mgr: failed to symlink {}: {e}", entry.name);
        }
    }
}
