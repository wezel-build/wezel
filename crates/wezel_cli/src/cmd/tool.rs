use std::fs;
use std::path::Path;

use anyhow::Context as _;
use toml_edit::{DocumentMut, Item, Table, value};

use crate::style;

#[derive(Debug, PartialEq, Eq)]
enum AddOutcome {
    Added,
    AlreadyPresent,
}

fn normalize_github_repository(repository: &str) -> anyhow::Result<String> {
    let url = url::Url::parse(repository)
        .with_context(|| format!("invalid GitHub repository URL `{repository}`"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        anyhow::bail!(
            "invalid GitHub repository URL `{repository}`; expected https://github.com/owner/repo"
        );
    }

    let path = url.path().strip_suffix('/').unwrap_or(url.path());
    let mut segments = path.trim_start_matches('/').split('/');
    let owner = segments.next().unwrap_or_default();
    let repo = segments.next().unwrap_or_default();
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    if owner.is_empty()
        || repo.is_empty()
        || segments.next().is_some()
        || !owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        || !repo
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        anyhow::bail!(
            "invalid GitHub repository URL `{repository}`; expected https://github.com/owner/repo"
        );
    }

    Ok(format!("{owner}/{repo}"))
}

fn table<'a>(item: &'a mut Item, name: &str) -> anyhow::Result<&'a mut Table> {
    item.as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("`{name}` in .wezel/config.toml must be a table"))
}

fn add_declaration(project_dir: &Path, name: &str, repository: &str) -> anyhow::Result<AddOutcome> {
    let path = project_dir.join(".wezel").join("config.toml");
    let raw = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut config = raw
        .parse::<DocumentMut>()
        .with_context(|| format!("parsing {}", path.display()))?;

    let tools = table(
        config
            .entry("tools")
            .or_insert_with(|| Item::Table(Table::new())),
        "tools",
    )?;
    let foragers = table(
        tools
            .entry("foragers")
            .or_insert_with(|| Item::Table(Table::new())),
        "tools.foragers",
    )?;

    if let Some(existing) = foragers.get(name) {
        let github = existing
            .get("github")
            .and_then(Item::as_str)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "tool `{name}` already exists in .wezel/config.toml without a valid `github` source"
                )
            })?;
        if github == repository {
            return Ok(AddOutcome::AlreadyPresent);
        }
        anyhow::bail!(
            "tool `{name}` already exists with GitHub source `{github}`; refusing to replace it with `{repository}`"
        );
    }

    let mut source = Table::new();
    source.insert("github", value(repository));
    foragers.insert(name, Item::Table(source));
    fs::write(&path, config.to_string()).with_context(|| format!("writing {}", path.display()))?;
    Ok(AddOutcome::Added)
}

pub fn tool_add_cmd(project_dir: &Path, name: &str, repository_url: &str) -> anyhow::Result<()> {
    let repository = normalize_github_repository(repository_url)?;
    let outcome = add_declaration(project_dir, name, &repository)?;
    match outcome {
        AddOutcome::Added => println!(
            "{} tool `{name}` from github.com/{repository}",
            style::success("Added")
        ),
        AddOutcome::AlreadyPresent => println!(
            "{} tool `{name}` already uses github.com/{repository}; syncing.",
            style::success("Unchanged:")
        ),
    }

    let tool_store = wezel_bench::Workspace::default_tool_store()?;
    let workspace = wezel_bench::Workspace::discover(project_dir.to_path_buf(), tool_store)?;
    crate::tool_sync(&workspace)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT_ID: &str = "416d4a8a-4f25-48a3-897e-14697a5a9afa";

    fn project(config: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".wezel")).unwrap();
        fs::write(dir.path().join(".wezel/config.toml"), config).unwrap();
        dir
    }

    #[test]
    fn normalizes_github_repository_urls() {
        assert_eq!(
            normalize_github_repository("https://github.com/wezel-build/wezel_filesize").unwrap(),
            "wezel-build/wezel_filesize"
        );
        assert_eq!(
            normalize_github_repository("https://github.com/wezel-build/wezel_filesize.git/")
                .unwrap(),
            "wezel-build/wezel_filesize"
        );
    }

    #[test]
    fn rejects_non_github_and_non_repository_urls() {
        for invalid in [
            "wezel-build/wezel_filesize",
            "http://github.com/wezel-build/wezel_filesize",
            "https://example.com/wezel-build/wezel_filesize",
            "https://github.com/wezel-build",
            "https://github.com/wezel-build/wezel_filesize/issues",
            "https://github.com/wezel-build/wezel_filesize?tab=readme",
        ] {
            assert!(
                normalize_github_repository(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn adds_tool_without_changing_unrelated_configuration() {
        let config = format!(
            "# project comment\nproject_id = \"{PROJECT_ID}\"\nname = \"demo\"\ncustom = \"preserve me\"\n\n[tools]\ntargets = [\"aarch64-apple-darwin\"] # target comment\n\n[unrelated]\nanswer = 42\n"
        );
        let dir = project(&config);

        assert_eq!(
            add_declaration(dir.path(), "filesize", "wezel-build/wezel_filesize").unwrap(),
            AddOutcome::Added
        );
        let updated = fs::read_to_string(dir.path().join(".wezel/config.toml")).unwrap();
        assert!(updated.contains("# project comment"));
        assert!(updated.contains("targets = [\"aarch64-apple-darwin\"] # target comment"));
        assert!(updated.contains("[unrelated]\nanswer = 42"));
        let parsed: toml::Value = toml::from_str(&updated).unwrap();
        assert_eq!(
            parsed["tools"]["foragers"]["filesize"]["github"].as_str(),
            Some("wezel-build/wezel_filesize")
        );
    }

    #[test]
    fn identical_declaration_is_idempotent_and_conflict_is_unchanged() {
        let config = format!(
            "project_id = \"{PROJECT_ID}\"\nname = \"demo\"\n\n[tools.foragers.filesize]\ngithub = \"wezel-build/wezel_filesize\"\ntag = \"v1\"\n"
        );
        let dir = project(&config);

        assert_eq!(
            add_declaration(dir.path(), "filesize", "wezel-build/wezel_filesize").unwrap(),
            AddOutcome::AlreadyPresent
        );
        assert_eq!(
            fs::read_to_string(dir.path().join(".wezel/config.toml")).unwrap(),
            config
        );

        let error = add_declaration(dir.path(), "filesize", "other/filesize").unwrap_err();
        assert!(error.to_string().contains("refusing to replace"));
        assert_eq!(
            fs::read_to_string(dir.path().join(".wezel/config.toml")).unwrap(),
            config
        );
    }

    #[test]
    fn add_reloads_config_and_calls_sync_boundary() {
        let config = format!("project_id = \"{PROJECT_ID}\"\nname = \"demo\"\n");
        let dir = project(&config);

        let error = tool_add_cmd(
            dir.path(),
            "filesize",
            "https://github.com/wezel-build/wezel_filesize",
        )
        .unwrap_err();

        assert!(error.to_string().contains("no targets declared"));
        let config = wezel_bench::ProjectConfig::load(dir.path()).unwrap();
        assert_eq!(
            config.tools.foragers["filesize"].github,
            "wezel-build/wezel_filesize"
        );
    }
}
