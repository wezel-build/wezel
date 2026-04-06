use std::io::Read;
use std::path::{Path, PathBuf};

// ── Trait ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("user declined to install `{plugin}`")]
    Declined { plugin: String },
    #[error("plugin `{plugin}` not available for target `{target}`")]
    NotAvailable { plugin: String, target: String },
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// Strategy for fetching missing forager plugins at runtime.
///
/// Implementations live in `wezel_cli`; the trait is defined here so
/// `invoke_forager` can accept `Option<&dyn PluginFetcher>`.
pub trait PluginFetcher {
    /// Fetch and install the plugin binary named `forager-{name}`.
    /// Returns the path to the installed binary.
    fn fetch(&self, name: &str) -> Result<PathBuf, FetchError>;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Target triple for the current platform.
pub fn current_target() -> Option<&'static str> {
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

/// Directory containing the wezel binary — plugins install here as siblings.
pub fn plugin_install_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
}

/// Extract a named binary from a `.tar.gz` archive and write it atomically to `dest`.
pub fn extract_and_install(
    archive_bytes: &[u8],
    binary_name: &str,
    dest: &Path,
) -> Result<(), FetchError> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let gz = GzDecoder::new(archive_bytes);
    let mut archive = Archive::new(gz);

    for entry in archive.entries().map_err(|e| FetchError::Other(e.into()))? {
        let mut entry = entry.map_err(|e| FetchError::Other(e.into()))?;
        let path = entry.path().map_err(|e| FetchError::Other(e.into()))?;
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name == binary_name {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| FetchError::Other(e.into()))?;
            }
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| FetchError::Other(e.into()))?;
            let tmp = dest.with_extension("tmp");
            std::fs::write(&tmp, &bytes).map_err(|e| FetchError::Other(e.into()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&tmp)
                    .map_err(|e| FetchError::Other(e.into()))?
                    .permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&tmp, perms).map_err(|e| FetchError::Other(e.into()))?;
            }
            std::fs::rename(&tmp, dest).map_err(|e| FetchError::Other(e.into()))?;
            return Ok(());
        }
    }
    Err(FetchError::Other(anyhow::anyhow!(
        "binary '{binary_name}' not found in archive"
    )))
}
