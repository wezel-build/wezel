use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{ProjectConfig, ToolsConfig};

const DEFAULT_GITIGNORE: &str = "\
# Wezel project-local state. Add patterns here as needed.
events/
*.local.toml
";

fn dot_wezel(project_dir: &Path) -> PathBuf {
    project_dir.join(".wezel")
}

fn config_path(project_dir: &Path) -> PathBuf {
    dot_wezel(project_dir).join("config.toml")
}

fn create_config(project_dir: &Path, server_url: Option<&str>) -> anyhow::Result<ProjectConfig> {
    let server_url = match server_url {
        Some(url) => Some(url.to_string()),
        None => prompt_server_url()?,
    };

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
        server_url,
        username: None,
        pheromone_dir: None,
        queue_dir: None,
        registries: None,
        data_branch: None,
        tools: ToolsConfig { targets },
    })
}

pub fn init_cmd(project_dir: &Path, server_url: Option<&str>) -> anyhow::Result<()> {
    let path = config_path(project_dir);

    let config = if path.exists() {
        let raw = fs::read_to_string(&path)?;
        let config: ProjectConfig = toml::from_str(&raw)?;
        println!("Using existing {}", path.display());
        config
    } else {
        let config = create_config(project_dir, server_url)?;
        let contents = toml::to_string_pretty(&config)?;
        fs::create_dir_all(dot_wezel(project_dir))?;
        fs::write(&path, &contents)?;
        // The .gitignore is part of the initial scaffold; written alongside
        // config.toml so first-run state is reproducible across machines.
        fs::write(dot_wezel(project_dir).join(".gitignore"), DEFAULT_GITIGNORE)?;
        println!("Created {}", path.display());
        config
    };

    // Register the project with the server (if configured).
    if let Some(ref server_url) = config.server_url {
        let upstream = crate::detect_upstream().unwrap_or_default();
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(10))
            .build();
        match agent
            .post(&format!("{server_url}/api/project"))
            .send_json(serde_json::json!({
                "uuid": config.project_id.to_string(),
                "name": config.name,
                "upstream": upstream,
            })) {
            Ok(_) => println!("Registered project with {server_url}"),
            Err(e) => log::warn!("Failed to register project with server: {e}"),
        }
    }

    Ok(())
}

fn prompt_server_url() -> anyhow::Result<Option<String>> {
    let url: String = dialoguer::Input::new()
        .with_prompt("Server URL (leave empty for standalone mode)")
        .allow_empty(true)
        .interact_text()?;

    let url = url.trim().to_string();
    if url.is_empty() {
        Ok(None)
    } else {
        Ok(Some(url))
    }
}
