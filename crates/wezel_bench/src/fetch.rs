use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

// ── Trait ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("plugin `{plugin}` not available for target `{target}`")]
    NotAvailable { plugin: String, target: String },
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// Strategy for fetching missing forager plugins at runtime.
///
/// Implementations live in `wezel_cli`; the trait is defined here so
/// `invoke_forager` can accept `Option<&mut dyn PluginFetcher>`.
pub trait PluginFetcher {
    /// Fetch and install the plugin, then return its project-local executable
    /// alias.
    fn fetch(&mut self, name: &str) -> Result<PathBuf, FetchError>;
}

// ── Caching wrapper ──────────────────────────────────────────────────────────

enum CachedOutcome {
    Available(PathBuf),
    NotAvailable { plugin: String, target: String },
    Failed(String),
}

/// Wraps any [`PluginFetcher`] and memoises outcomes so each plugin is only
/// downloaded once per run, even when multiple callers try to fetch the same
/// plugin.
pub struct CachingFetcher<'a> {
    inner: &'a mut dyn PluginFetcher,
    cache: HashMap<String, CachedOutcome>,
}

impl<'a> CachingFetcher<'a> {
    pub fn new(inner: &'a mut dyn PluginFetcher) -> Self {
        Self {
            inner,
            cache: HashMap::new(),
        }
    }
}

impl<'a> PluginFetcher for CachingFetcher<'a> {
    fn fetch(&mut self, name: &str) -> Result<PathBuf, FetchError> {
        if let Some(outcome) = self.cache.get(name) {
            return match outcome {
                CachedOutcome::Available(path) => Ok(path.clone()),
                CachedOutcome::NotAvailable { plugin, target } => Err(FetchError::NotAvailable {
                    plugin: plugin.clone(),
                    target: target.clone(),
                }),
                CachedOutcome::Failed(msg) => Err(FetchError::Other(anyhow::anyhow!("{msg}"))),
            };
        }

        let result = self.inner.fetch(name);

        let outcome = match &result {
            Ok(path) => CachedOutcome::Available(path.clone()),
            Err(FetchError::NotAvailable { plugin, target }) => CachedOutcome::NotAvailable {
                plugin: plugin.clone(),
                target: target.clone(),
            },
            Err(FetchError::Other(e)) => CachedOutcome::Failed(e.to_string()),
        };
        self.cache.insert(name.to_string(), outcome);

        result
    }
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

/// Strip macOS Gatekeeper's quarantine xattr from a freshly downloaded file.
///
/// No-op on non-macOS targets and when the attribute isn't present.
pub fn strip_quarantine(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("xattr")
            .args(["-d", "com.apple.quarantine"])
            .arg(path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
    }
}

/// Extract the single executable from a `.tar.gz` or `.tar.xz` archive into
/// `dest_dir`, preserving its published file name.
pub fn extract_and_install(archive_bytes: &[u8], dest_dir: &Path) -> Result<PathBuf, FetchError> {
    use tar::Archive;

    // Detect format from magic bytes: XZ = fd 37 7a 58 5a 00, gzip = 1f 8b
    let is_xz = archive_bytes.starts_with(&[0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00]);

    fn install_from_tar<R: std::io::Read>(
        mut archive: Archive<R>,
        dest_dir: &Path,
    ) -> Result<PathBuf, FetchError> {
        let mut executable = None;
        for entry in archive.entries().map_err(|e| FetchError::Other(e.into()))? {
            let mut entry = entry.map_err(|e| FetchError::Other(e.into()))?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let path = entry.path().map_err(|e| FetchError::Other(e.into()))?;
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let mode = entry.header().mode().unwrap_or_default();
            if mode & 0o111 == 0 && !file_name.ends_with(".exe") {
                continue;
            }
            if executable.is_some() {
                return Err(FetchError::Other(anyhow::anyhow!(
                    "archive contains more than one executable"
                )));
            }
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| FetchError::Other(e.into()))?;
            executable = Some((file_name, bytes));
        }
        let (file_name, bytes) = executable
            .ok_or_else(|| FetchError::Other(anyhow::anyhow!("archive contains no executable")))?;
        std::fs::create_dir_all(dest_dir).map_err(|e| FetchError::Other(e.into()))?;
        let dest = dest_dir.join(file_name);
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
        std::fs::rename(&tmp, &dest).map_err(|e| FetchError::Other(e.into()))?;
        Ok(dest)
    }

    if is_xz {
        let xz = xz2::read::XzDecoder::new(archive_bytes);
        install_from_tar(Archive::new(xz), dest_dir)
    } else {
        let gz = flate2::read::GzDecoder::new(archive_bytes);
        install_from_tar(Archive::new(gz), dest_dir)
    }
}

/// Find the sole installed executable in a content-addressed store directory.
/// Old schema sidecars are ignored so existing installations can be linked
/// into a project without downloading the archive again.
pub fn installed_binary(dir: &Path) -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".schema.json"))
        })
        .collect();
    (candidates.len() == 1).then(|| candidates[0].clone())
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn archive(entries: &[(&str, u32)]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (name, mode) in entries {
            let body = format!("contents of {name}");
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(*mode);
            header.set_cksum();
            builder
                .append_data(&mut header, name, body.as_bytes())
                .unwrap();
        }
        let mut encoder = builder.into_inner().unwrap();
        encoder.flush().unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn extraction_preserves_the_publishers_binary_name() {
        let bytes = archive(&[("package/README", 0o644), ("package/acme-measure", 0o755)]);
        let dir = tempfile::tempdir().unwrap();

        let installed = extract_and_install(&bytes, dir.path()).unwrap();

        assert_eq!(installed, dir.path().join("acme-measure"));
        assert_eq!(
            std::fs::read_to_string(installed).unwrap(),
            "contents of package/acme-measure"
        );
    }

    #[test]
    fn extraction_rejects_ambiguous_executables() {
        let bytes = archive(&[("package/first", 0o755), ("package/second", 0o755)]);
        let dir = tempfile::tempdir().unwrap();

        let error = extract_and_install(&bytes, dir.path()).unwrap_err();

        assert!(error.to_string().contains("more than one executable"));
    }
}
