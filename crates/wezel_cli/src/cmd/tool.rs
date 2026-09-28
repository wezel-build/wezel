use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::style;

#[derive(Debug, PartialEq, Eq)]
enum AddOutcome {
    Added,
    AlreadyPresent,
}

const LOCAL_TOOLS_FILE: &str = "tools.local.toml";

#[derive(Default, Deserialize, Serialize)]
struct LocalToolsConfig {
    #[serde(default)]
    foragers: BTreeMap<String, PathBuf>,
}

fn local_tools_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".wezel").join(LOCAL_TOOLS_FILE)
}

fn load_local_tools(project_dir: &Path) -> anyhow::Result<LocalToolsConfig> {
    let path = local_tools_path(project_dir);
    if !path.is_file() {
        return Ok(LocalToolsConfig::default());
    }
    let raw = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
}

fn save_local_tools(project_dir: &Path, tools: &LocalToolsConfig) -> anyhow::Result<()> {
    let path = local_tools_path(project_dir);
    if tools.foragers.is_empty() {
        if path.is_file() {
            fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
        return Ok(());
    }
    let body = toml::to_string_pretty(tools).context("serialising local tool state")?;
    let temporary = path.with_file_name(format!(".tools.{}.local.toml", uuid::Uuid::new_v4()));
    fs::write(&temporary, body).with_context(|| format!("writing {}", temporary.display()))?;
    fs::rename(&temporary, &path).with_context(|| format!("writing {}", path.display()))
}

fn ensure_local_state_ignored(project_dir: &Path) -> anyhow::Result<()> {
    const ENTRY: &str = "*.local.toml";
    let path = project_dir.join(".wezel").join(".gitignore");
    let mut contents = fs::read_to_string(&path).unwrap_or_default();
    if contents.lines().any(|line| line.trim() == ENTRY) {
        return Ok(());
    }
    if !contents.is_empty() && !contents.ends_with('\n') {
        contents.push('\n');
    }
    contents.push_str(ENTRY);
    contents.push('\n');
    fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))
}

fn canonical_executable(path: &Path) -> anyhow::Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("executor path {} does not exist", path.display()))?;
    if !canonical.is_file() {
        anyhow::bail!("executor path {} is not a file", canonical.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if canonical.metadata()?.permissions().mode() & 0o111 == 0 {
            anyhow::bail!("executor path {} is not executable", canonical.display());
        }
    }
    Ok(canonical)
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
    if !wezel_bench::workspace::is_valid_tool_name(name) {
        anyhow::bail!(
            "invalid tool name `{name}`; use only letters, numbers, dots, hyphens, and underscores"
        );
    }
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

pub fn tool_link_cmd(project_dir: &Path, name: &str, executable: &Path) -> anyhow::Result<()> {
    if !wezel_bench::workspace::is_valid_tool_name(name) {
        anyhow::bail!(
            "invalid tool name `{name}`; use only letters, numbers, dots, hyphens, and underscores"
        );
    }
    let executable = canonical_executable(executable)?;
    crate::fetcher::read_schema(name, &executable)?;

    ensure_local_state_ignored(project_dir)?;
    let mut local = load_local_tools(project_dir)?;
    if let Some(existing) = local.foragers.get(name) {
        if existing != &executable {
            anyhow::bail!(
                "tool `{name}` is already linked to {}; run `wezel project tool unlink {name}` first",
                existing.display()
            );
        }
    } else {
        local.foragers.insert(name.to_string(), executable.clone());
        save_local_tools(project_dir, &local)?;
    }

    let tool_store = wezel_bench::Workspace::default_tool_store()?;
    let workspace = wezel_bench::Workspace::discover(project_dir.to_path_buf(), tool_store)?;
    crate::tool_sync(&workspace)?;
    println!(
        "{} local tool `{name}` to {}",
        style::success("Linked"),
        executable.display()
    );
    Ok(())
}

pub fn tool_unlink_cmd(project_dir: &Path, name: &str) -> anyhow::Result<()> {
    if !wezel_bench::workspace::is_valid_tool_name(name) {
        anyhow::bail!("invalid tool name `{name}`");
    }
    let mut local = load_local_tools(project_dir)?;
    let executable = local
        .foragers
        .remove(name)
        .ok_or_else(|| anyhow::anyhow!("tool `{name}` has no local link"))?;

    let tool_store = wezel_bench::Workspace::default_tool_store()?;
    let workspace = wezel_bench::Workspace::discover(project_dir.to_path_buf(), tool_store)?;
    let published = workspace.config.tools.foragers.contains_key(name);
    if let Some(alias) = workspace.local_executor_path(name) {
        let sidecar = wezel_bench::Workspace::schema_sidecar_path(&alias);
        if alias.symlink_metadata().is_ok() {
            fs::remove_file(&alias).with_context(|| format!("removing {}", alias.display()))?;
        }
        if sidecar.is_file() {
            fs::remove_file(&sidecar).with_context(|| format!("removing {}", sidecar.display()))?;
        }
    }
    save_local_tools(project_dir, &local)?;

    let workspace =
        wezel_bench::Workspace::discover(project_dir.to_path_buf(), workspace.tool_store)?;
    crate::tool_sync(&workspace)?;
    let restored = if published {
        " Published resolution is active."
    } else {
        ""
    };
    println!(
        "{} local tool `{name}` from {}.{restored}",
        style::success("Unlinked"),
        executable.display()
    );
    Ok(())
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

    #[cfg(unix)]
    fn fake_executor(dir: &Path, filename: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;

        let path = dir.join(filename);
        fs::write(
            &path,
            r#"#!/bin/sh
if [ "$1" = "--schema" ]; then
  printf '%s' '{"name":"publisher-name","description":"local test","inputs":{"type":"object","properties":{"message":{"type":"string"}},"required":["message"]},"outcomes_doc":""}'
else
  printf '%s' '{"outcomes":[{"name":"count","value":7}]}' > "$FORAGER_OUT"
fi
"#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
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
    fn rejects_tool_names_that_are_not_safe_executor_file_names() {
        let config = format!("project_id = \"{PROJECT_ID}\"\nname = \"demo\"\n");
        let dir = project(&config);

        let error =
            add_declaration(dir.path(), "../filesize", "wezel-build/wezel_filesize").unwrap_err();

        assert!(error.to_string().contains("invalid tool name"));
        assert_eq!(
            fs::read_to_string(dir.path().join(".wezel/config.toml")).unwrap(),
            config
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

    #[cfg(unix)]
    #[test]
    fn local_link_is_idempotent_usable_by_lint_and_removable() {
        let config = format!("project_id = \"{PROJECT_ID}\"\nname = \"demo\"\n");
        let dir = project(&config);
        fs::create_dir_all(dir.path().join(".wezel/experiments/local")).unwrap();
        fs::write(
            dir.path().join(".wezel/experiments/local/experiment.toml"),
            "description = \"local\"\n[step.dev.sample]\nmessage = \"hello\"\n",
        )
        .unwrap();
        let executable = fake_executor(dir.path(), "fake-forager");
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "test@example.com"][..],
            &["config", "user.name", "Test"][..],
            &["add", "."][..],
            &["commit", "-q", "-m", "init"][..],
        ] {
            assert!(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(dir.path())
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        }

        tool_link_cmd(dir.path(), "dev", &executable).unwrap();
        tool_link_cmd(dir.path(), "dev", &executable).unwrap();

        let store = tempfile::tempdir().unwrap();
        let workspace =
            wezel_bench::Workspace::discover(dir.path().into(), store.path().into()).unwrap();
        assert_eq!(
            workspace.local_tool_path("dev"),
            Some(executable.canonicalize().unwrap().as_path())
        );
        assert!(workspace.resolve_plugin("dev").is_some());
        assert!(!dir.path().join(".wezel/wezel.lock").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join(".wezel/config.toml")).unwrap(),
            config
        );
        assert!(
            fs::read_to_string(dir.path().join(".wezel/.gitignore"))
                .unwrap()
                .lines()
                .any(|line| line == "*.local.toml")
        );

        wezel_bench::lint::run_lint(&workspace, None).unwrap();
        let run = wezel_bench::run::run_experiment("local", &workspace, None, None).unwrap();
        assert_eq!(run.steps[0].measurements[0].value, serde_json::json!(7));

        tool_unlink_cmd(dir.path(), "dev").unwrap();
        let workspace =
            wezel_bench::Workspace::discover(dir.path().into(), store.path().into()).unwrap();
        assert!(!workspace.has_local_tool("dev"));
        assert!(workspace.resolve_plugin("dev").is_none());
        assert!(!local_tools_path(dir.path()).exists());
    }

    #[cfg(unix)]
    #[test]
    fn local_link_overrides_published_declaration_without_changing_it() {
        let config = format!(
            "project_id = \"{PROJECT_ID}\"\nname = \"demo\"\n\n[tools.foragers.dev]\ngithub = \"acme/published\"\n"
        );
        let dir = project(&config);
        let executable = fake_executor(dir.path(), "fake-forager");

        tool_link_cmd(dir.path(), "dev", &executable).unwrap();

        assert_eq!(
            fs::read_to_string(dir.path().join(".wezel/config.toml")).unwrap(),
            config
        );
        assert!(!dir.path().join(".wezel/wezel.lock").exists());
        let local = load_local_tools(dir.path()).unwrap();
        assert_eq!(
            local.foragers.get("dev"),
            Some(&executable.canonicalize().unwrap())
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_link_rejects_unsafe_names_non_executables_and_conflicts() {
        let config = format!("project_id = \"{PROJECT_ID}\"\nname = \"demo\"\n");
        let dir = project(&config);
        let executable = fake_executor(dir.path(), "first");
        let other = fake_executor(dir.path(), "second");
        let not_executable = dir.path().join("not-executable");
        fs::write(&not_executable, "no").unwrap();
        let invalid_schema = dir.path().join("invalid-schema");
        fs::write(&invalid_schema, "#!/bin/sh\nprintf 'not json'\n").unwrap();
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&invalid_schema, fs::Permissions::from_mode(0o755)).unwrap();

        assert!(tool_link_cmd(dir.path(), "../dev", &executable).is_err());
        assert!(tool_link_cmd(dir.path(), "dev", &not_executable).is_err());
        let error = tool_link_cmd(dir.path(), "dev", &invalid_schema).unwrap_err();
        assert!(error.to_string().contains("invalid output"));
        tool_link_cmd(dir.path(), "dev", &executable).unwrap();
        let error = tool_link_cmd(dir.path(), "dev", &other).unwrap_err();
        assert!(error.to_string().contains("unlink"));
    }
}
