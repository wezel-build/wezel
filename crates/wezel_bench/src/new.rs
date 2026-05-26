use std::path::Path;

use anyhow::{Result, bail};

pub fn create_experiment(name: &str, description: Option<&str>, project_dir: &Path) -> Result<()> {
    let experiment_dir = project_dir.join(".wezel").join("experiments").join(name);

    if experiment_dir.exists() {
        bail!(
            "experiment '{name}' already exists at {}",
            experiment_dir.display()
        );
    }

    std::fs::create_dir_all(&experiment_dir)?;

    let mut toml_content = if let Some(d) = description {
        format!("description = \"{d}\"\n")
    } else {
        String::default()
    };
    toml_content.push_str(
        r#"
[step.exec.your-step-name]
cmd = "cargo build"
"#,
    );

    std::fs::write(experiment_dir.join("experiment.toml"), toml_content)?;

    eprintln!("Created experiment at {}", experiment_dir.display());
    eprintln!(
        "  Edit {}/experiment.toml to configure steps",
        experiment_dir.display()
    );

    Ok(())
}
