use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use wezel_bench::Workspace;
use wezel_bench::fetch::{self, FetchError, PluginFetcher};
use wezel_bench::lockfile::{self, LockedTool, WezelLock};

/// Resolves and installs forager binaries from sources declared in
/// `.wezel/config.toml`'s `[tools.foragers.<name>]` table, pinning resolved
/// tags and per-target archive hashes in `.wezel/wezel.lock`.
///
/// Never prompts. Quarantine xattrs are stripped after install on macOS.
pub struct ConfigFetcher<'ws> {
    workspace: &'ws Workspace,
    lock: WezelLock,
    /// When true, installation is allowed only for foragers already pinned in
    /// the lockfile, and `wezel.lock` is never written. Used by lint.
    read_only: bool,
}

impl<'ws> ConfigFetcher<'ws> {
    pub fn new(workspace: &'ws Workspace) -> anyhow::Result<Self> {
        let lock = lockfile::load(&workspace.project_dir)?;
        Ok(Self {
            workspace,
            lock,
            read_only: false,
        })
    }

    /// Lint flavour: refuses to install anything not already locked, and
    /// never mutates `wezel.lock`.
    pub fn read_only(workspace: &'ws Workspace) -> anyhow::Result<Self> {
        let lock = lockfile::load(&workspace.project_dir)?;
        Ok(Self {
            workspace,
            lock,
            read_only: true,
        })
    }

    /// Record the asset hash for `(name, target)` in `wezel.lock` without
    /// downloading the binary. Used by `tool sync` to make the lockfile
    /// platform-complete in a single run — the cargo-dist `.tar.xz.sha256`
    /// sidecar is ~80 bytes vs. several MB for the full archive.
    ///
    /// No-op when the entry already exists at the resolved tag.
    pub fn lock_target(&mut self, name: &str, target: &str) -> anyhow::Result<()> {
        let source = self
            .workspace
            .config
            .tools
            .foragers
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("forager `{name}` not declared in [tools.foragers]"))?;

        let locked = self.lock.tools.foragers.get(name).cloned();
        let resolved = resolve_release(
            &source.github,
            source.tag.as_deref(),
            locked.as_ref(),
            name,
            target,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;

        if let Some(l) = &locked
            && l.tag == resolved.tag
            && l.assets.contains_key(target)
        {
            return Ok(());
        }

        let sha_url = format!("{}.sha256", resolved.download_url);
        let body = http_get_bytes(&sha_url, name)
            .map_err(|e| anyhow::anyhow!("fetching {sha_url}: {e}"))?;
        let text =
            std::str::from_utf8(&body).map_err(|_| anyhow::anyhow!("{sha_url}: non-utf8 body"))?;
        let hex = text
            .split_whitespace()
            .next()
            .filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| anyhow::anyhow!("{sha_url}: unexpected sha256 format"))?
            .to_lowercase();
        let lock_key = format!("sha256:{hex}");

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
        lockfile::save(&self.workspace.project_dir, &self.lock)?;

        eprintln!(
            "{} {}",
            crate::style::stderr_success(format!("Locked `{name}` ({target})")),
            crate::style::stderr_muted(format!(
                "at {} from github.com/{}",
                resolved.tag, source.github
            ))
        );
        Ok(())
    }
}

impl<'ws> PluginFetcher for ConfigFetcher<'ws> {
    fn fetch(&mut self, name: &str) -> Result<PathBuf, FetchError> {
        if !wezel_bench::workspace::is_valid_tool_name(name) {
            return Err(FetchError::Other(anyhow::anyhow!(
                "invalid tool name `{name}`; use only letters, numbers, dots, hyphens, and underscores"
            )));
        }
        let target = fetch::current_target().ok_or_else(|| FetchError::NotAvailable {
            plugin: name.to_string(),
            target: "unknown".into(),
        })?;

        let source = self
            .workspace
            .config
            .tools
            .foragers
            .get(name)
            .ok_or_else(|| {
                FetchError::Other(anyhow::anyhow!(
                    "forager `{name}` not declared in `.wezel/config.toml`. \
                 Add `[tools.foragers.{name}]` with `github = \"owner/repo\"`."
                ))
            })?;

        let locked = self.lock.tools.foragers.get(name).cloned();

        if self.read_only && locked.is_none() {
            return Err(FetchError::Other(anyhow::anyhow!(
                "forager `{name}` is not pinned in wezel.lock; \
                 cannot install in read-only mode (run `wezel experiment run` \
                 to refresh the lockfile)"
            )));
        }

        if let Some(archive_sha) = locked
            .as_ref()
            .and_then(|locked| locked.assets.get(target))
            .map(|key| key.strip_prefix("sha256:").unwrap_or(key))
        {
            let install_dir = self.workspace.install_dir(archive_sha);
            if let Some(binary) = fetch::installed_binary(&install_dir) {
                let executor = link_executor(self.workspace, name, &binary)?;
                write_schema_sidecar(name, &executor)?;
                return Ok(executor);
            }
        }

        // Priority for the tag: lockfile > config pin > latest release.
        let resolved = resolve_release(
            &source.github,
            source.tag.as_deref(),
            locked.as_ref(),
            name,
            target,
        )?;

        let bytes = http_get_bytes(&resolved.download_url, name)?;
        let archive_sha = sha256_hex(&bytes);
        let lock_key = format!("sha256:{archive_sha}");

        if let Some(expected) = locked.as_ref().and_then(|l| l.assets.get(target))
            && expected != &lock_key
        {
            return Err(FetchError::Other(anyhow::anyhow!(
                "wezel.lock sha mismatch for {name} ({target}): \
                     expected {expected}, got {lock_key}. \
                     Delete .wezel/wezel.lock to refresh."
            )));
        }

        let install_dir = self.workspace.install_dir(&archive_sha);
        let binary = fetch::extract_and_install(&bytes, &install_dir)?;
        fetch::strip_quarantine(&binary);
        let executor = link_executor(self.workspace, name, &binary)?;
        write_schema_sidecar(name, &executor)?;
        eprintln!(
            "{} {}",
            crate::style::stderr_success(format!("Installed `{name}`")),
            crate::style::stderr_muted(format!(
                "({}) from github.com/{} to {}",
                resolved.tag,
                source.github,
                binary.display()
            ))
        );

        if !self.read_only {
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
            lockfile::save(&self.workspace.project_dir, &self.lock).map_err(FetchError::Other)?;
        }

        Ok(executor)
    }
}

#[derive(Debug)]
struct ResolvedRelease {
    tag: String,
    download_url: String,
}

fn resolve_release(
    repo: &str,
    config_tag: Option<&str>,
    locked: Option<&LockedTool>,
    name: &str,
    target: &str,
) -> Result<ResolvedRelease, FetchError> {
    let pinned = locked.map(|l| l.tag.as_str()).or(config_tag);
    let release = match pinned {
        Some(tag) => fetch_release_by_tag(repo, tag)?,
        None => fetch_latest_release(repo)?,
    };

    resolve_release_metadata(&release, name, target)
}

fn resolve_release_metadata(
    release: &serde_json::Value,
    name: &str,
    target: &str,
) -> Result<ResolvedRelease, FetchError> {
    let tag = release["tag_name"]
        .as_str()
        .ok_or_else(|| FetchError::Other(anyhow::anyhow!("release has no tag_name")))?
        .to_string();

    let assets = release["assets"]
        .as_array()
        .ok_or_else(|| FetchError::Other(anyhow::anyhow!("release has no assets")))?;

    let suffixes = [format!("-{target}.tar.gz"), format!("-{target}.tar.xz")];
    let matching: Vec<_> = assets
        .iter()
        .filter(|asset| {
            let name = asset["name"].as_str().unwrap_or_default();
            suffixes.iter().any(|suffix| {
                name.strip_suffix(suffix)
                    .is_some_and(|prefix| !prefix.is_empty())
            })
        })
        .collect();
    let asset = match matching.as_slice() {
        [asset] => *asset,
        [] => {
            return Err(FetchError::NotAvailable {
                plugin: name.into(),
                target: target.into(),
            });
        }
        _ => {
            return Err(FetchError::Other(anyhow::anyhow!(
                "release has multiple archives for target `{target}`"
            )));
        }
    };

    let download_url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| FetchError::Other(anyhow::anyhow!("asset has no download URL")))?
        .to_string();

    Ok(ResolvedRelease { tag, download_url })
}

/// Fetch the most recent release. Uses `/releases?per_page=1` rather than
/// `/releases/latest` because the latter skips prereleases — and forager
/// repos commonly tag everything as `nightly-*` prereleases.
fn fetch_latest_release(repo: &str) -> Result<serde_json::Value, FetchError> {
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=1");
    let value = github_get_json(&url)?;
    let mut releases = value.as_array().cloned().ok_or_else(|| {
        FetchError::Other(anyhow::anyhow!("GET {url}: expected array, got {value}"))
    })?;
    if releases.is_empty() {
        return Err(FetchError::Other(anyhow::anyhow!(
            "no releases published on github.com/{repo}"
        )));
    }
    Ok(releases.remove(0))
}

fn fetch_release_by_tag(repo: &str, tag: &str) -> Result<serde_json::Value, FetchError> {
    let url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
    github_get_json(&url)
}

/// GitHub auth token for API + asset requests. Prefers `GH_TOKEN` (the `gh`
/// CLI's convention, and what wezel's own GitHub Action sets); falls back to
/// the now-deprecated `GITHUB_TOKEN` for back-compat with older setups.
fn github_token() -> Option<String> {
    for var in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(token) = std::env::var(var) {
            let token = token.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    None
}

fn github_get_json(url: &str) -> Result<serde_json::Value, FetchError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();
    let mut req = agent.get(url).set("User-Agent", "wezel-cli");
    if let Some(token) = github_token() {
        req = req.set("Authorization", &format!("Bearer {token}"));
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
    if let Some(token) = github_token() {
        req = req.set("Authorization", &format!("Bearer {token}"));
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

fn link_executor(workspace: &Workspace, name: &str, binary: &Path) -> Result<PathBuf, FetchError> {
    let link = workspace.executor_path(name).ok_or_else(|| {
        FetchError::Other(anyhow::anyhow!("invalid project executor name `{name}`"))
    })?;
    let parent = link
        .parent()
        .ok_or_else(|| FetchError::Other(anyhow::anyhow!("executor path has no parent")))?;
    std::fs::create_dir_all(parent).map_err(|e| FetchError::Other(e.into()))?;
    let binary = binary
        .canonicalize()
        .map_err(|e| FetchError::Other(e.into()))?;
    let temporary = parent.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));

    #[cfg(unix)]
    std::os::unix::fs::symlink(&binary, &temporary).map_err(|e| FetchError::Other(e.into()))?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&binary, &temporary)
        .map_err(|e| FetchError::Other(e.into()))?;

    #[cfg(windows)]
    if std::fs::symlink_metadata(&link).is_ok() {
        std::fs::remove_file(&link).map_err(|e| FetchError::Other(e.into()))?;
    }
    if let Err(error) = std::fs::rename(&temporary, &link) {
        let _ = std::fs::remove_file(&temporary);
        return Err(FetchError::Other(error.into()));
    }
    Ok(link)
}

/// Run `<binary> --schema` once at install time and write the JSON to the
/// project-local schema sidecar. The project alias replaces the publisher's
/// schema name so one global binary can have different names across projects.
fn write_schema_sidecar(forager_name: &str, binary: &std::path::Path) -> Result<(), FetchError> {
    let out = std::process::Command::new(binary)
        .arg("--schema")
        .output()
        .map_err(|e| {
            FetchError::Other(anyhow::anyhow!("running --schema for {forager_name}: {e}"))
        })?;
    if !out.status.success() {
        return Err(FetchError::Other(anyhow::anyhow!(
            "{forager_name} --schema exited with {}",
            out.status
        )));
    }
    let mut parsed: wezel_types::ForagerSchema =
        serde_json::from_slice(&out.stdout).map_err(|e| {
            FetchError::Other(anyhow::anyhow!(
                "{forager_name} --schema produced invalid output: {e}"
            ))
        })?;
    parsed.name = forager_name.to_string();
    let schema_path = Workspace::schema_sidecar_path(binary);
    let body = serde_json::to_vec_pretty(&parsed).map_err(|e| FetchError::Other(e.into()))?;
    std::fs::write(&schema_path, body).map_err(|e| {
        FetchError::Other(anyhow::anyhow!("writing {}: {e}", schema_path.display()))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TARGET: &str = "aarch64-apple-darwin";

    fn release(assets: &[&str]) -> serde_json::Value {
        json!({
            "tag_name": "v1.0.0",
            "assets": assets.iter().map(|name| json!({
                "name": name,
                "browser_download_url": format!("https://example.invalid/{name}"),
            })).collect::<Vec<_>>()
        })
    }

    #[test]
    fn release_archive_name_is_independent_of_project_alias() {
        let release = release(&[
            "publisher-tool-aarch64-apple-darwin.tar.xz.sha256",
            "publisher-tool-x86_64-unknown-linux-gnu.tar.xz",
            "publisher-tool-aarch64-apple-darwin.tar.xz",
        ]);
        let resolved = resolve_release_metadata(&release, "my-alias", TARGET).unwrap();
        assert_eq!(resolved.tag, "v1.0.0");
        assert_eq!(
            resolved.download_url,
            "https://example.invalid/publisher-tool-aarch64-apple-darwin.tar.xz"
        );
    }

    #[test]
    fn multiple_archives_for_one_target_are_ambiguous() {
        let release = release(&[
            "first-aarch64-apple-darwin.tar.xz",
            "second-aarch64-apple-darwin.tar.gz",
        ]);
        let error = resolve_release_metadata(&release, "alias", TARGET).unwrap_err();
        assert!(error.to_string().contains("multiple archives"));
    }

    #[test]
    fn checksums_and_other_targets_are_not_installable_archives() {
        let release = release(&[
            "wezel_cargo-aarch64-apple-darwin.tar.xz.sha256",
            "wezel_cargo-x86_64-unknown-linux-gnu.tar.xz",
        ]);
        assert!(matches!(
            resolve_release_metadata(&release, "cargo", TARGET),
            Err(FetchError::NotAvailable { plugin, .. }) if plugin == "cargo"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn project_alias_links_to_unchanged_global_name_and_owns_schema_name() {
        use std::os::unix::fs::PermissionsExt as _;

        let project = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join(".wezel")).unwrap();
        std::fs::write(
            project.path().join(".wezel/config.toml"),
            format!(
                "project_id = \"{}\"\nname = \"test\"\n",
                uuid::Uuid::new_v4()
            ),
        )
        .unwrap();
        let binary_dir = store.path().join("hash");
        std::fs::create_dir(&binary_dir).unwrap();
        let binary = binary_dir.join("publishers-own-name");
        std::fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s' '{\"name\":\"publisher-name\",\"description\":\"test\",\"inputs\":{},\"outcomes_doc\":\"\"}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let workspace = Workspace::discover(project.path().into(), store.path().into()).unwrap();

        let executor = link_executor(&workspace, "local-alias", &binary).unwrap();
        write_schema_sidecar("local-alias", &executor).unwrap();

        assert_eq!(
            std::fs::read_link(&executor).unwrap(),
            binary.canonicalize().unwrap()
        );
        assert!(binary.is_file());
        let schema: wezel_types::ForagerSchema = serde_json::from_slice(
            &std::fs::read(Workspace::schema_sidecar_path(&executor)).unwrap(),
        )
        .unwrap();
        assert_eq!(schema.name, "local-alias");
    }

    #[cfg(unix)]
    #[test]
    fn locked_global_install_is_reused_through_a_project_alias() {
        use std::os::unix::fs::PermissionsExt as _;

        let project = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join(".wezel")).unwrap();
        std::fs::write(
            project.path().join(".wezel/config.toml"),
            format!(
                "project_id = \"{}\"\nname = \"test\"\n\n[tools.foragers.size-check]\ngithub = \"wezel-build/wezel_filesize\"\n",
                uuid::Uuid::new_v4()
            ),
        )
        .unwrap();

        let target = fetch::current_target().unwrap();
        let hash = "ab".repeat(32);
        let mut lock = WezelLock {
            version: lockfile::CURRENT_VERSION,
            ..WezelLock::default()
        };
        lock.tools.foragers.insert(
            "size-check".into(),
            LockedTool {
                github: "wezel-build/wezel_filesize".into(),
                tag: "v1.0.0".into(),
                assets: BTreeMap::from([(target.into(), format!("sha256:{hash}"))]),
            },
        );
        lockfile::save(project.path(), &lock).unwrap();

        let binary_dir = store.path().join(&hash);
        std::fs::create_dir(&binary_dir).unwrap();
        let binary = binary_dir.join("wezel_filesize");
        std::fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s' '{\"name\":\"filesize\",\"description\":\"test\",\"inputs\":{},\"outcomes_doc\":\"\"}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();

        let workspace = Workspace::discover(project.path().into(), store.path().into()).unwrap();
        let executor = ConfigFetcher::new(&workspace)
            .unwrap()
            .fetch("size-check")
            .unwrap();

        assert_eq!(
            std::fs::read_link(&executor).unwrap(),
            binary.canonicalize().unwrap()
        );
        let schema: wezel_types::ForagerSchema = serde_json::from_slice(
            &std::fs::read(Workspace::schema_sidecar_path(&executor)).unwrap(),
        )
        .unwrap();
        assert_eq!(schema.name, "size-check");
    }
}
