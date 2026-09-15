use std::path::Path;

use anyhow::Result;

use crate::style;
use wezel_bench::{Workspace, lint, lockfile};

pub fn status_cmd(project_dir: &Path) -> Result<()> {
    let tool_store = Workspace::default_tool_store()?;
    let ws = Workspace::discover(project_dir.to_path_buf(), tool_store)?;
    let config_path = ws.project_dir.join(".wezel").join("config.toml");

    println!(
        "project:  {} ({})",
        style::strong(&ws.config.name),
        style::muted(ws.config.project_id)
    );
    println!("config:   {}", style::muted(config_path.display()));
    let fiflok = std::env::var("WEZEL_API_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|url| style::success(format!("server {url} (WEZEL_API_URL)")))
        .unwrap_or_else(|| style::warning("(WEZEL_API_URL not set)"));
    println!("fiflok:   {fiflok}");

    let lock = lockfile::load(&ws.project_dir)?;
    let lockfile_present = lockfile::path(&ws.project_dir).is_file();

    println!();
    println!(
        "{}",
        style::strong(format!("foragers ({}):", ws.config.tools.foragers.len()))
    );
    if ws.config.tools.foragers.is_empty() {
        println!("  {}", style::muted("(none declared in [tools.foragers])"));
    }
    for (name, source) in &ws.config.tools.foragers {
        let installed = ws.resolve_plugin(name).is_some();
        let locked = lock.tools.foragers.get(name);
        let mark = if installed {
            style::success("✓")
        } else {
            style::failure("✗")
        };
        let version = match locked {
            Some(t) => style::muted(format!(" @ {}", t.tag)),
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
        let note = if note.is_empty() {
            note
        } else {
            style::warning(note)
        };
        println!(
            "  {mark} {} ({}{}){}",
            style::strong(format!("{name:<12}")),
            style::muted(&source.github),
            version,
            note
        );
    }

    println!();
    if lockfile_present {
        let declared: std::collections::BTreeSet<_> = ws.config.tools.foragers.keys().collect();
        let locked_set: std::collections::BTreeSet<_> = lock.tools.foragers.keys().collect();
        let missing: Vec<_> = declared.difference(&locked_set).collect();
        if missing.is_empty() {
            println!(
                "lockfile: {} wezel.lock present, all declared foragers locked",
                style::success("✓")
            );
        } else {
            let status = format!(
                "wezel.lock missing entries for: {}",
                missing
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            println!(
                "lockfile: {} {}",
                style::failure("✗"),
                style::failure(status)
            );
        }
    } else if ws.config.tools.foragers.is_empty() {
        println!(
            "lockfile: {}",
            style::muted("(no wezel.lock — no foragers declared)")
        );
    } else {
        println!(
            "lockfile: {} {}",
            style::failure("✗"),
            style::failure("wezel.lock missing — run `wezel project tool sync`")
        );
    }

    if ws.config.tools.foragers.is_empty() {
        println!("schema:   {}", style::muted("(n/a — no foragers declared)"));
    } else if lint::bundle_is_stale(&ws) {
        println!(
            "schema:   {} {}",
            style::failure("✗"),
            style::failure(".wezel/schema.json stale — run `wezel project tool sync`")
        );
    } else {
        println!(
            "schema:   {} .wezel/schema.json up to date",
            style::success("✓")
        );
    }

    Ok(())
}
