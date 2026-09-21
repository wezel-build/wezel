use std::fs;
use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{ProjectConfig, ToolsConfig};
use crate::style;

const DEFAULT_APP_URL: &str = "https://app.wezel.build";

const DEFAULT_GITIGNORE: &str = "\
# Wezel project-local state. Add patterns here as needed.
runs/
tools/
*.local.toml
";

fn dot_wezel(project_dir: &Path) -> PathBuf {
    project_dir.join(".wezel")
}

fn config_path(project_dir: &Path) -> PathBuf {
    dot_wezel(project_dir).join("config.toml")
}

struct Repository {
    upstream: String,
    subdir: String,
}

fn git_output(project_dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project_dir)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Produce the forge-shaped `host/owner/repo` identity used by Farfocel.
fn normalize_upstream(remote: &str) -> String {
    let without_scheme = remote
        .trim()
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(remote.trim());
    let without_user = without_scheme
        .rsplit_once('@')
        .map(|(_, rest)| rest)
        .unwrap_or(without_scheme);
    let upstream = match (without_user.find(':'), without_user.find('/')) {
        (Some(colon), slash) if slash.is_none_or(|slash| colon < slash) => {
            format!("{}/{}", &without_user[..colon], &without_user[colon + 1..])
        }
        _ => without_user.to_string(),
    };
    upstream
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_string()
}

fn detect_repository(project_dir: &Path) -> Option<Repository> {
    let upstream = normalize_upstream(&git_output(project_dir, &["remote", "get-url", "origin"])?);
    let root = PathBuf::from(git_output(project_dir, &["rev-parse", "--show-toplevel"])?);
    let project = project_dir.canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    let subdir = project
        .strip_prefix(root)
        .ok()?
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    Some(Repository { upstream, subdir })
}

fn onboarding_url(
    app_url: &str,
    config: &ProjectConfig,
    repository: &Repository,
) -> anyhow::Result<url::Url> {
    let mut url = url::Url::parse(app_url)?;
    url.set_path("/projects/create");
    url.set_query(None);
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("upstream", &repository.upstream)
            .append_pair("name", &config.name)
            .append_pair("project_id", &config.project_id.to_string());
        if !repository.subdir.is_empty() {
            query.append_pair("subdir", &repository.subdir);
        }
    }
    Ok(url)
}

fn open_browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(url).status();
    #[cfg(target_os = "windows")]
    let result = Command::new("cmd").args(["/C", "start", "", url]).status();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(url).status();

    result.is_ok_and(|status| status.success())
}

fn create_config(project_dir: &Path) -> anyhow::Result<ProjectConfig> {
    let default_name = project_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string());

    let mut prompt = dialoguer::Input::<String>::new().with_prompt("Project name");
    if let Some(ref d) = default_name {
        prompt = prompt.default(d.clone());
    }
    let name: String = prompt.interact_text()?;
    let name = name.trim().to_string();
    if name.is_empty() {
        anyhow::bail!("project name cannot be empty");
    }

    let mut targets = indexmap::IndexSet::new();
    if let Some(t) = wezel_bench::fetch::current_target() {
        targets.insert(t.to_string());
    }

    Ok(ProjectConfig {
        project_id: uuid::Uuid::new_v4(),
        name,
        registries: None,
        tools: ToolsConfig { targets },
    })
}

pub fn init_cmd(project_dir: &Path) -> anyhow::Result<()> {
    let path = config_path(project_dir);

    let config = if path.exists() {
        let raw = fs::read_to_string(&path)?;
        let config: ProjectConfig = toml::from_str(&raw)?;
        println!(
            "{} {}",
            style::warning("Using existing"),
            style::muted(path.display())
        );
        config
    } else {
        let config = create_config(project_dir)?;
        let contents = toml::to_string_pretty(&config)?;
        fs::create_dir_all(dot_wezel(project_dir))?;
        fs::write(&path, &contents)?;
        // The .gitignore is part of the initial scaffold; written alongside
        // config.toml so first-run state is reproducible across machines.
        fs::write(dot_wezel(project_dir).join(".gitignore"), DEFAULT_GITIGNORE)?;
        println!(
            "{} {}",
            style::success("Created"),
            style::muted(path.display())
        );
        config
    };

    if std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && let Some(repository) = detect_repository(project_dir)
        && dialoguer::Confirm::new()
            .with_prompt("Open Wezel and start tracking this project?")
            .default(true)
            .interact()?
    {
        let app_url =
            std::env::var("WEZEL_APP_URL").unwrap_or_else(|_| DEFAULT_APP_URL.to_string());
        let url = onboarding_url(&app_url, &config, &repository)?;
        if !open_browser(url.as_str()) {
            println!("Open this URL to finish setup:\n{url}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_tools_are_ignored_by_default() {
        assert!(DEFAULT_GITIGNORE.lines().any(|line| line == "tools/"));
        assert!(!DEFAULT_GITIGNORE.lines().any(|line| line == "executors/"));
    }

    #[test]
    fn normalizes_https_and_scp_remotes() {
        assert_eq!(
            normalize_upstream("https://github.com/acme/widget.git"),
            "github.com/acme/widget"
        );
        assert_eq!(
            normalize_upstream("git@github.com:acme/widget.git"),
            "github.com/acme/widget"
        );
        assert_eq!(
            normalize_upstream("ssh://git@git.acme.test/acme/widget.git"),
            "git.acme.test/acme/widget"
        );
    }

    #[test]
    fn onboarding_url_carries_local_project_identity() {
        let config = ProjectConfig {
            project_id: uuid::Uuid::parse_str("416d4a8a-4f25-48a3-897e-14697a5a9afa").unwrap(),
            name: "compiler tools".into(),
            registries: None,
            tools: ToolsConfig::default(),
        };
        let repository = Repository {
            upstream: "github.com/acme/widget".into(),
            subdir: "crates/compiler".into(),
        };

        let url = onboarding_url("https://app.wezel.build", &config, &repository).unwrap();
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(url.path(), "/projects/create");
        assert_eq!(query["upstream"], "github.com/acme/widget");
        assert_eq!(query["name"], "compiler tools");
        assert_eq!(query["project_id"], "416d4a8a-4f25-48a3-897e-14697a5a9afa");
        assert_eq!(query["subdir"], "crates/compiler");
    }
}
