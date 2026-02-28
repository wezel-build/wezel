use std::collections::BTreeMap;
use std::fs;

use serde::{Deserialize, Serialize};

use crate::shell::{Shell, ensure_shell_hook, sync_init_script};
use crate::wezel_dir;

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
                println!("Shell hook is set up. No aliases configured yet.");
            } else {
                println!(
                    "Shell hook is set up. {} alias(es) active:",
                    aliases.aliases.len()
                );
                for (k, v) in &aliases.aliases {
                    println!("  {k} -> pheromone-{v}");
                }
            }
        }
        Some(name) => {
            if remove {
                if aliases.aliases.remove(name).is_some() {
                    save_aliases(&aliases)?;
                    sync_init_script(shell, &aliases.aliases)?;
                    println!("Removed alias `{name}`.");
                } else {
                    println!("No alias `{name}` found.");
                }
            } else {
                let handler = handler.unwrap_or(name);
                ensure_shell_hook(shell)?;
                aliases
                    .aliases
                    .insert(name.to_string(), handler.to_string());
                save_aliases(&aliases)?;
                sync_init_script(shell, &aliases.aliases)?;
                println!("Alias `{name}` -> pheromone-{handler}");
            }
        }
    }

    Ok(())
}
