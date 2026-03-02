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

pub fn setup_cmd(burrow_url: Option<&str>) -> anyhow::Result<()> {
    let path = config_path();

    if path.exists() {
        anyhow::bail!(
            ".wezel/config.toml already exists in this directory. \
             Edit it directly or remove it first."
        );
    }

    let burrow_url = match burrow_url {
        Some(url) => url.to_string(),
        None => prompt_burrow_url()?,
    };

    let config = ProjectConfig {
        burrow_url: Some(burrow_url),
        username: None,
    };

    let contents = toml::to_string_pretty(&config)?;
    fs::create_dir_all(dot_wezel())?;
    fs::write(&path, &contents)?;

    println!("Created {}", path.display());
    Ok(())
}

fn prompt_burrow_url() -> anyhow::Result<String> {
    let url: String = dialoguer::Input::new()
        .with_prompt("Burrow API URL")
        .interact_text()?;

    let url = url.trim().to_string();
    if url.is_empty() {
        anyhow::bail!("burrow_url cannot be empty");
    }
    Ok(url)
}
