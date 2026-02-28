use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

const HOOK_MARKER: &str = "# >>> wezel pheromone >>>";
const HOOK_END: &str = "# <<< wezel pheromone <<<";

fn hook_block() -> String {
    format!(
        r#"{HOOK_MARKER}
__wezel_preexec() {{
  pheromone_cli pre "$1"
}}

__wezel_precmd() {{
  (pheromone_cli post "$?" &) 2>/dev/null
}}

preexec_functions+=(__wezel_preexec)
precmd_functions+=(__wezel_precmd)
{HOOK_END}"#
    )
}

#[derive(Parser)]
#[command(name = "pheromone", about = "Lightweight build observer")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install shell hooks into your .zshrc
    Init,
    Post {
        args: Vec<String>,
    },
    Pre {
        args: Vec<String>,
    },
    Update,
}

fn zshrc_path() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".zshrc")
}

fn init() -> anyhow::Result<()> {
    let path = zshrc_path();

    let contents = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        String::new()
    };

    if contents.contains(HOOK_MARKER) {
        println!("Hook already installed in {}", path.display());
        return Ok(());
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;

    use std::io::Write;
    writeln!(file)?;
    writeln!(file, "{}", hook_block())?;

    println!("Installed hook in {}", path.display());
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => init(),
        Command::Pre { .. } => anyhow::Ok(()),
        _ => anyhow::Ok(()),
    }
}
