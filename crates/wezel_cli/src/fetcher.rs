use std::io::Read;
use std::path::PathBuf;

use wezel_bench::fetch::{self, FetchError, PluginFetcher};

// ── Burrow fetcher ──────────────────────────────────────────────────────────

pub struct BurrowFetcher {
    server_url: String,
    auto_yes: bool,
}

impl BurrowFetcher {
    pub fn new(server_url: String, auto_yes: bool) -> Self {
        Self { server_url, auto_yes }
    }
}

impl PluginFetcher for BurrowFetcher {
    fn fetch(&self, name: &str) -> Result<PathBuf, FetchError> {
        let binary_name = format!("forager-{name}");
        let target = fetch::current_target().ok_or_else(|| FetchError::NotAvailable {
            plugin: binary_name.clone(),
            target: "unknown".into(),
        })?;

        if !self.auto_yes {
            let confirmed = dialoguer::Confirm::new()
                .with_prompt(format!(
                    "Plugin `{binary_name}` not found. Install from {}?",
                    self.server_url
                ))
                .default(true)
                .interact()
                .map_err(|e| FetchError::Other(e.into()))?;
            if !confirmed {
                return Err(FetchError::Declined { plugin: binary_name });
            }
        } else {
            eprintln!("Installing plugin `{binary_name}` from {}...", self.server_url);
        }

        let url = format!(
            "{}/api/tools/{binary_name}/binary/{target}",
            self.server_url.trim_end_matches('/')
        );
        let agent = ureq::Agent::new();
        let resp = agent
            .get(&url)
            .call()
            .map_err(|e| FetchError::Other(anyhow::anyhow!("downloading {binary_name}: {e}")))?;

        let mut bytes = Vec::new();
        resp.into_reader()
            .read_to_end(&mut bytes)
            .map_err(|e| FetchError::Other(e.into()))?;

        let dest_dir = fetch::plugin_install_dir()
            .ok_or_else(|| FetchError::Other(anyhow::anyhow!("cannot determine install directory")))?;
        let dest = dest_dir.join(&binary_name);

        fetch::extract_and_install(&bytes, &binary_name, &dest)?;
        eprintln!("Installed `{binary_name}` to {}", dest.display());
        Ok(dest)
    }
}

// ── GitHub fetcher ──────────────────────────────────────────────────────────

pub struct GithubFetcher {
    repo: String,
    auto_yes: bool,
}

impl GithubFetcher {
    pub fn new(repo: impl Into<String>, auto_yes: bool) -> Self {
        Self {
            repo: repo.into(),
            auto_yes,
        }
    }
}

impl PluginFetcher for GithubFetcher {
    fn fetch(&self, name: &str) -> Result<PathBuf, FetchError> {
        let binary_name = format!("forager-{name}");
        let target = fetch::current_target().ok_or_else(|| FetchError::NotAvailable {
            plugin: binary_name.clone(),
            target: "unknown".into(),
        })?;

        if !self.auto_yes {
            let confirmed = dialoguer::Confirm::new()
                .with_prompt(format!(
                    "Plugin `{binary_name}` not found. Install from github.com/{}?",
                    self.repo
                ))
                .default(true)
                .interact()
                .map_err(|e| FetchError::Other(e.into()))?;
            if !confirmed {
                return Err(FetchError::Declined { plugin: binary_name });
            }
        } else {
            eprintln!("Installing plugin `{binary_name}` from github.com/{}...", self.repo);
        }

        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(30))
            .build();

        // Find latest nightly release.
        let releases_url = format!("https://api.github.com/repos/{}/releases", self.repo);
        let mut req = agent.get(&releases_url).query("per_page", "10");
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            let token = token.trim().to_string();
            if !token.is_empty() {
                req = req.set("Authorization", &format!("Bearer {token}"));
            }
        }
        req = req.set("User-Agent", "wezel-cli");

        let releases: Vec<serde_json::Value> = req
            .call()
            .map_err(|e| FetchError::Other(anyhow::anyhow!("fetching releases: {e}")))?
            .into_json()
            .map_err(|e| FetchError::Other(e.into()))?;

        let release = releases
            .iter()
            .find(|r| {
                r.get("tag_name")
                    .and_then(|v| v.as_str())
                    .is_some_and(|t| t.starts_with("nightly-"))
            })
            .ok_or_else(|| {
                FetchError::Other(anyhow::anyhow!(
                    "no nightly release found in {}",
                    self.repo
                ))
            })?;

        // Build asset lookup.
        let gh_assets = release["assets"]
            .as_array()
            .ok_or_else(|| FetchError::Other(anyhow::anyhow!("release has no assets")))?;

        // Find an archive that matches the binary name and target.
        // Archive naming convention: {name}-{version}-{target}.tar.gz
        let asset = gh_assets
            .iter()
            .find(|a| {
                let fname = a["name"].as_str().unwrap_or("");
                fname.contains(&binary_name) && fname.contains(target) && !fname.ends_with(".sha256")
            })
            .ok_or_else(|| FetchError::NotAvailable {
                plugin: binary_name.clone(),
                target: target.to_string(),
            })?;

        let download_url = asset["browser_download_url"]
            .as_str()
            .ok_or_else(|| FetchError::Other(anyhow::anyhow!("asset has no download URL")))?;

        let mut dl_req = agent.get(download_url).set("User-Agent", "wezel-cli");
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            let token = token.trim().to_string();
            if !token.is_empty() {
                dl_req = dl_req.set("Authorization", &format!("Bearer {token}"));
            }
        }

        let resp = dl_req
            .call()
            .map_err(|e| FetchError::Other(anyhow::anyhow!("downloading {binary_name}: {e}")))?;

        let mut bytes = Vec::new();
        resp.into_reader()
            .read_to_end(&mut bytes)
            .map_err(|e| FetchError::Other(e.into()))?;

        let dest_dir = fetch::plugin_install_dir()
            .ok_or_else(|| FetchError::Other(anyhow::anyhow!("cannot determine install directory")))?;
        let dest = dest_dir.join(&binary_name);

        fetch::extract_and_install(&bytes, &binary_name, &dest)?;
        eprintln!("Installed `{binary_name}` to {}", dest.display());
        Ok(dest)
    }
}
