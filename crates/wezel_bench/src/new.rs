use std::path::Path;

use anyhow::{Result, bail};

pub fn create_benchmark(name: &str, description: Option<&str>, project_dir: &Path) -> Result<()> {
    let benchmarks_dir = project_dir.join(".wezel").join("benchmarks");
    let bench_dir = benchmarks_dir.join(name);

    if bench_dir.exists() {
        bail!("benchmark '{name}' already exists at {}", bench_dir.display());
    }

    std::fs::create_dir_all(&bench_dir)?;

    let mut toml_content = format!("name = \"{name}\"\n");
    if let Some(d) = description {
        toml_content.push_str(&format!("description = \"{d}\"\n"));
    }
    toml_content.push_str(
        r#"
[[steps]]
name = "build"
cmd = "cargo build"
"#,
    );

    std::fs::write(bench_dir.join("benchmark.toml"), toml_content)?;

    eprintln!("Created benchmark at {}", bench_dir.display());
    eprintln!("  Edit {}/benchmark.toml to configure steps", bench_dir.display());

    Ok(())
}
