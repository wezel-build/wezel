use std::collections::BTreeMap;
use std::fs;

use serde::{Deserialize, Serialize};

use crate::pheromones_dir;
use crate::shell::{Shell, ensure_shell_hook, sync_init_script};
use crate::style;
use crate::wezel_dir;

fn warn_missing_handler(handler: &str) {
    let path = pheromones_dir().join(format!("pheromone-{handler}"));
    if !path.is_file() {
        eprintln!(
            "{}",
            style::stderr_warning(format!(
                "warning: pheromone-{handler} not found in {}",
                pheromones_dir().display()
            ))
        );
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AliasesFile {
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
}

fn aliases_toml_path() -> std::path::PathBuf {
    wezel_dir().join("aliases.toml")
}

pub fn load_aliases() -> anyhow::Result<AliasesFile> {
    let path = aliases_toml_path();
    if !path.exists() {
        return Ok(AliasesFile::default());
    }
    let contents = fs::read_to_string(&path)?;
    let file: AliasesFile = toml::from_str(&contents)?;
    Ok(file)
}

fn save_aliases(file: &AliasesFile) -> anyhow::Result<()> {
    let dir = wezel_dir();
    fs::create_dir_all(&dir)?;
    let contents = toml::to_string_pretty(file)?;
    fs::write(aliases_toml_path(), contents)?;
    Ok(())
}

pub fn alias_cmd(name: Option<&str>, handler: Option<&str>, remove: bool) -> anyhow::Result<()> {
    let shell = Shell::detect()
        .ok_or_else(|| anyhow::anyhow!("Could not detect shell from $SHELL env var"))?;

    let mut aliases = load_aliases()?;

    match name {
        None => {
            ensure_shell_hook(shell)?;
            sync_init_script(shell, &aliases.aliases)?;
            if aliases.aliases.is_empty() {
                println!(
                    "{} {}",
                    style::success("Shell hook is set up."),
                    style::muted("No aliases configured yet.")
                );
            } else {
                println!(
                    "{} {}",
                    style::success("Shell hook is set up."),
                    style::strong(format!("{} alias(es) active:", aliases.aliases.len()))
                );
                for (k, v) in &aliases.aliases {
                    println!(
                        "  {} {} pheromone-{}",
                        style::strong(k),
                        style::muted("->"),
                        v
                    );
                }
            }
        }
        Some(name) => {
            if remove {
                if aliases.aliases.remove(name).is_some() {
                    save_aliases(&aliases)?;
                    sync_init_script(shell, &aliases.aliases)?;
                    println!("{}", style::success(format!("Removed alias `{name}`.")));
                } else {
                    println!("{}", style::warning(format!("No alias `{name}` found.")));
                }
            } else {
                let handler = handler.unwrap_or(name);
                warn_missing_handler(handler);
                ensure_shell_hook(shell)?;
                aliases
                    .aliases
                    .insert(name.to_string(), handler.to_string());
                save_aliases(&aliases)?;
                sync_init_script(shell, &aliases.aliases)?;
                println!(
                    "{} {} pheromone-{}",
                    style::success(format!("Alias `{name}`")),
                    style::muted("->"),
                    handler
                );
            }
        }
    }

    Ok(())
}
