use std::io::Read;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("plugin `{plugin}` not available for target `{target}`")]
    NotAvailable { plugin: String, target: String },
    #[error("{0}")]
    Other(#[from] anyhow::Error),
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

/// Extract a named binary from a `.tar.gz` or `.tar.xz` archive and write it atomically to `dest`.
pub fn extract_and_install(
    archive_bytes: &[u8],
    binary_name: &str,
    dest: &Path,
) -> Result<(), FetchError> {
    use tar::Archive;

    // Detect format from magic bytes: XZ = fd 37 7a 58 5a 00, gzip = 1f 8b
    let is_xz = archive_bytes.starts_with(&[0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00]);

    fn install_from_tar<R: std::io::Read>(
        mut archive: Archive<R>,
        binary_name: &str,
        dest: &Path,
    ) -> Result<(), FetchError> {
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
                    std::fs::set_permissions(&tmp, perms)
                        .map_err(|e| FetchError::Other(e.into()))?;
                }
                std::fs::rename(&tmp, dest).map_err(|e| FetchError::Other(e.into()))?;
                return Ok(());
            }
        }
        Err(FetchError::Other(anyhow::anyhow!(
            "binary '{binary_name}' not found in archive"
        )))
    }

    if is_xz {
        let xz = xz2::read::XzDecoder::new(archive_bytes);
        install_from_tar(Archive::new(xz), binary_name, dest)
    } else {
        let gz = flate2::read::GzDecoder::new(archive_bytes);
        install_from_tar(Archive::new(gz), binary_name, dest)
    }
}
