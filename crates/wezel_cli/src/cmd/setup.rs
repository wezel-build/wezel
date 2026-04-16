use std::fs;
use std::path::PathBuf;

use crate::config::ProjectConfig;

fn dot_wezel() -> PathBuf {
    std::env::current_dir()
        .expect("could not determine current directory")
        .join(".wezel")
}

fn config_path() -> PathBuf {
    dot_wezel().join("config.toml")
}

fn create_config(server_url: Option<&str>) -> anyhow::Result<ProjectConfig> {
    let server_url = match server_url {
        Some(url) => Some(url.to_string()),
        None => prompt_server_url()?,
    };

    let default_name = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));

    let mut prompt = dialoguer::Input::<String>::new().with_prompt("Project name");
    if let Some(ref d) = default_name {
        prompt = prompt.default(d.clone());
    }
    let name: String = prompt.interact_text()?;
    let name = name.trim().to_string();
    if name.is_empty() {
        anyhow::bail!("project name cannot be empty");
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
    })
}

pub fn setup_cmd(server_url: Option<&str>) -> anyhow::Result<()> {
    let path = config_path();

    let config = if path.exists() {
        let raw = fs::read_to_string(&path)?;
        let config: ProjectConfig = toml::from_str(&raw)?;
        println!("Using existing {}", path.display());
        config
    } else {
        let config = create_config(server_url)?;
        let contents = toml::to_string_pretty(&config)?;
        fs::create_dir_all(dot_wezel())?;
        fs::write(&path, &contents)?;
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
