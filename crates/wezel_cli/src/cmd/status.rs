use std::path::Path;

use anyhow::Result;

use wezel_bench::{Workspace, lint, lockfile};

pub fn status_cmd(project_dir: &Path) -> Result<()> {
    let plugin_dir = Workspace::default_plugin_dir()?;
    let ws = Workspace::discover(project_dir.to_path_buf(), plugin_dir)?;
    let config_path = ws.project_dir.join(".wezel").join("config.toml");

    println!("project:  {} ({})", ws.config.name, ws.config.project_id);
    println!("config:   {}", config_path.display());
    let target = match (
        ws.config.target.server_url(),
        ws.config.target.data_branch(),
    ) {
        (Some(url), _) => format!("server {url}"),
        (_, Some(branch)) => format!("data branch {branch}"),
        _ => "(none)".to_string(),
    };
    println!("target:   {target}");

    let lock = lockfile::load(&ws.project_dir)?;
    let lockfile_present = lockfile::path(&ws.project_dir).is_file();

    println!();
    println!("foragers ({}):", ws.config.tools.foragers.len());
    if ws.config.tools.foragers.is_empty() {
        println!("  (none declared in [tools.foragers])");
    }
    for (name, source) in &ws.config.tools.foragers {
        let installed = ws.resolve_plugin(name).is_some();
        let locked = lock.tools.foragers.get(name);
        let mark = if installed { "✓" } else { "✗" };
        let version = match locked {
            Some(t) => format!(" @ {}", t.tag),
            None => String::new(),
        };
        let note = match (installed, locked) {
            (true, Some(_)) => String::new(),
            (true, None) => " (installed but not in wezel.lock)".to_string(),
            (false, Some(_)) => {
                " (locked but not installed — run `wezel project tool sync`)".to_string()
            }
            (false, None) => {
                " (declared but not installed — run `wezel project tool sync`)".to_string()
            }
        };
        println!("  {mark} {name:<12} ({}{version}){note}", source.github);
    }

    println!();
    if lockfile_present {
        let declared: std::collections::BTreeSet<_> = ws.config.tools.foragers.keys().collect();
        let locked_set: std::collections::BTreeSet<_> = lock.tools.foragers.keys().collect();
        let missing: Vec<_> = declared.difference(&locked_set).collect();
        if missing.is_empty() {
            println!("lockfile: ✓ wezel.lock present, all declared foragers locked");
        } else {
            println!(
                "lockfile: ✗ wezel.lock missing entries for: {}",
                missing
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    } else if ws.config.tools.foragers.is_empty() {
        println!("lockfile: (no wezel.lock — no foragers declared)");
    } else {
        println!("lockfile: ✗ wezel.lock missing — run `wezel project tool sync`");
    }

    if ws.config.tools.foragers.is_empty() {
        println!("schema:   (n/a — no foragers declared)");
    } else if lint::bundle_is_stale(&ws) {
        println!("schema:   ✗ .wezel/schema.json stale — run `wezel project tool sync`");
    } else {
        println!("schema:   ✓ .wezel/schema.json up to date");
    }

    Ok(())
}
