use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use wezel_bench::Config;
use wezel_bench::fetch::{self, FetchError, PluginFetcher};
use wezel_bench::lockfile::{self, LockedTool, WezelLock};

/// Resolves and installs forager binaries from sources declared in
/// `.wezel/config.toml`'s `[tools.foragers.<name>]` table, pinning resolved
/// tags and per-target archive hashes in `.wezel/wezel.lock`.
///
/// Never prompts. Quarantine xattrs are stripped after install on macOS.
pub struct ConfigFetcher {
    project_dir: PathBuf,
    config: Config,
    lock: WezelLock,
}

impl ConfigFetcher {
    pub fn new(project_dir: PathBuf, config: Config) -> anyhow::Result<Self> {
        let lock = lockfile::load(&project_dir)?;
        Ok(Self {
            project_dir,
            config,
            lock,
        })
    }
}

impl PluginFetcher for ConfigFetcher {
    fn fetch(&mut self, name: &str) -> Result<PathBuf, FetchError> {
        let binary_name = format!("forager-{name}");
        let target = fetch::current_target().ok_or_else(|| FetchError::NotAvailable {
            plugin: binary_name.clone(),
            target: "unknown".into(),
        })?;

        let source = self.config.tools.foragers.get(name).ok_or_else(|| {
            FetchError::Other(anyhow::anyhow!(
                "forager `{name}` not declared in `.wezel/config.toml`. \
                 Add `[tools.foragers.{name}]` with `github = \"owner/repo\"`."
            ))
        })?;

        let locked = self.lock.tools.foragers.get(name).cloned();

        // Priority for the tag: lockfile > config pin > latest release.
        let resolved = resolve_release(
            &source.github,
            source.tag.as_deref(),
            locked.as_ref(),
            &binary_name,
            target,
        )?;

        let bytes = http_get_bytes(&resolved.download_url, &binary_name)?;
        let archive_sha = sha256_hex(&bytes);
        let lock_key = format!("sha256:{archive_sha}");

        if let Some(expected) = locked.as_ref().and_then(|l| l.assets.get(target)) {
            if expected != &lock_key {
                return Err(FetchError::Other(anyhow::anyhow!(
                    "wezel.lock sha mismatch for {binary_name} ({target}): \
                     expected {expected}, got {lock_key}. \
                     Delete .wezel/wezel.lock to refresh."
                )));
            }
        }

        let dest_dir = fetch::plugin_install_dir().ok_or_else(|| {
            FetchError::Other(anyhow::anyhow!("cannot determine install directory"))
        })?;
        let dest = dest_dir.join(&binary_name);
        fetch::extract_and_install(&bytes, &binary_name, &dest)?;
        fetch::strip_quarantine(&dest);
        eprintln!(
            "Installed `{binary_name}` ({}) from github.com/{} to {}",
            resolved.tag,
            source.github,
            dest.display()
        );

        if self.lock.version == 0 {
            self.lock.version = lockfile::CURRENT_VERSION;
        }
        let entry = self
            .lock
            .tools
            .foragers
            .entry(name.to_string())
            .or_insert_with(|| LockedTool {
                github: source.github.clone(),
                tag: resolved.tag.clone(),
                assets: BTreeMap::new(),
            });
        entry.github = source.github.clone();
        entry.tag = resolved.tag.clone();
        entry.assets.insert(target.to_string(), lock_key);
        lockfile::save(&self.project_dir, &self.lock).map_err(FetchError::Other)?;

        Ok(dest)
    }
}

struct ResolvedRelease {
    tag: String,
    download_url: String,
}

fn resolve_release(
    repo: &str,
    config_tag: Option<&str>,
    locked: Option<&LockedTool>,
    binary_name: &str,
    target: &str,
) -> Result<ResolvedRelease, FetchError> {
    let pinned = locked.map(|l| l.tag.as_str()).or(config_tag);
    let release = match pinned {
        Some(tag) => fetch_release_by_tag(repo, tag)?,
        None => fetch_latest_release(repo)?,
    };

    let tag = release["tag_name"]
        .as_str()
        .ok_or_else(|| FetchError::Other(anyhow::anyhow!("release has no tag_name")))?
        .to_string();

    let assets = release["assets"]
        .as_array()
        .ok_or_else(|| FetchError::Other(anyhow::anyhow!("release has no assets")))?;

    // Archive naming convention: {name}-{version}-{target}.tar.gz; cargo-dist
    // uses the crate name (underscores) while the binary uses hyphens, so
    // accept both forms.
    let underscored = binary_name.replace('-', "_");
    let asset = assets
        .iter()
        .find(|a| {
            let fname = a["name"].as_str().unwrap_or("");
            (fname.contains(binary_name) || fname.contains(&underscored))
                && fname.contains(target)
                && !fname.ends_with(".sha256")
        })
        .ok_or_else(|| FetchError::NotAvailable {
            plugin: binary_name.into(),
            target: target.into(),
        })?;

    let download_url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| FetchError::Other(anyhow::anyhow!("asset has no download URL")))?
        .to_string();

    Ok(ResolvedRelease { tag, download_url })
}

fn fetch_latest_release(repo: &str) -> Result<serde_json::Value, FetchError> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    github_get_json(&url)
}

fn fetch_release_by_tag(repo: &str, tag: &str) -> Result<serde_json::Value, FetchError> {
    let url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
    github_get_json(&url)
}

fn github_get_json(url: &str) -> Result<serde_json::Value, FetchError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();
    let mut req = agent.get(url).set("User-Agent", "wezel-cli");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
    }
    let resp = req
        .call()
        .map_err(|e| FetchError::Other(anyhow::anyhow!("GET {url}: {e}")))?;
    resp.into_json()
        .map_err(|e| FetchError::Other(anyhow::anyhow!("decoding {url}: {e}")))
}

fn http_get_bytes(url: &str, binary_name: &str) -> Result<Vec<u8>, FetchError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(120))
        .build();
    let mut req = agent.get(url).set("User-Agent", "wezel-cli");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
    }
    let resp = req
        .call()
        .map_err(|e| FetchError::Other(anyhow::anyhow!("downloading {binary_name}: {e}")))?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| FetchError::Other(e.into()))?;
    Ok(bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}
