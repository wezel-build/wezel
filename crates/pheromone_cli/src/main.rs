use clap::{Parser, Subcommand};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

const FLUSH_LOCK: &str = ".flush.lock";
const SOURCE_MARKER: &str = "# >>> wezel pheromone >>>";
const SOURCE_END: &str = "# <<< wezel pheromone <<<";

fn wezel_dir() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".wezel")
}

fn init_zsh_path() -> PathBuf {
    wezel_dir().join("init.zsh")
}

fn zshrc_path() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".zshrc")
}

fn pheromone_dir() -> PathBuf {
    dirs::data_local_dir()
        .expect("could not determine local data directory")
        .join("pheromone")
}

fn events_dir() -> PathBuf {
    pheromone_dir().join("events")
}

fn source_block() -> String {
    let path = init_zsh_path();
    let path = path.display();
    format!("{SOURCE_MARKER}\n[[ -f \"{path}\" ]] && source \"{path}\"\n{SOURCE_END}")
}

fn alias_line(tool: &str) -> String {
    format!("alias {tool}=\"pheromone_cli exec -- {tool}\"")
}

// ── Init ─────────────────────────────────────────────────────────────────────

fn init() -> anyhow::Result<()> {
    let zshrc = zshrc_path();
    let contents = if zshrc.exists() {
        fs::read_to_string(&zshrc)?
    } else {
        String::new()
    };

    if contents.contains(SOURCE_MARKER) {
        println!("Already sourced in {}", zshrc.display());
        return Ok(());
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&zshrc)?;

    writeln!(file)?;
    writeln!(file, "{}", source_block())?;

    // Ensure ~/.wezel/init.zsh exists.
    ensure_init_zsh()?;

    println!("Installed source hook in {}", zshrc.display());
    Ok(())
}

fn ensure_init_zsh() -> anyhow::Result<()> {
    let dir = wezel_dir();
    fs::create_dir_all(&dir)?;
    let path = init_zsh_path();
    if !path.exists() {
        fs::write(
            &path,
            "# Managed by pheromone — edit freely, but keep the marker comments.\n",
        )?;
    }
    Ok(())
}

// ── Alias ────────────────────────────────────────────────────────────────────

fn alias_install(tool: &str) -> anyhow::Result<()> {
    ensure_init_zsh()?;
    let path = init_zsh_path();
    let contents = fs::read_to_string(&path)?;
    let line = alias_line(tool);

    if contents.contains(&line) {
        println!("Alias for `{tool}` already present in {}", path.display());
        return Ok(());
    }

    let mut file = fs::OpenOptions::new().append(true).open(&path)?;
    writeln!(file, "{line}")?;

    println!("Added alias for `{tool}` in {}", path.display());
    Ok(())
}

fn alias_remove(tool: &str) -> anyhow::Result<()> {
    let path = init_zsh_path();
    if !path.exists() {
        println!("No alias for `{tool}` found (init.zsh does not exist)");
        return Ok(());
    }

    let contents = fs::read_to_string(&path)?;
    let line = alias_line(tool);

    if !contents.contains(&line) {
        println!("No alias for `{tool}` found in {}", path.display());
        return Ok(());
    }

    let output: String = contents
        .lines()
        .filter(|l| *l != line)
        .collect::<Vec<_>>()
        .join("\n");

    let mut final_contents = output.trim_end_matches('\n').to_string();
    if !final_contents.is_empty() {
        final_contents.push('\n');
    }

    fs::write(&path, final_contents)?;
    println!("Removed alias for `{tool}` from {}", path.display());
    Ok(())
}

// ── Flush machinery ──────────────────────────────────────────────────────────

struct FlushLock {
    path: PathBuf,
}

impl FlushLock {
    fn try_acquire(dir: &std::path::Path) -> Option<Self> {
        let path = dir.join(FLUSH_LOCK);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => Some(Self { path }),
            Err(_) => None,
        }
    }
}

impl Drop for FlushLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn flush_events() -> anyhow::Result<()> {
    let events_dir = events_dir();
    if !events_dir.exists() {
        return Ok(());
    }

    let Some(_lock) = FlushLock::try_acquire(&events_dir) else {
        return Ok(());
    };

    let entries: Vec<PathBuf> = fs::read_dir(&events_dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .collect();

    if entries.is_empty() {
        return Ok(());
    }

    let mut events: Vec<serde_json::Value> = Vec::with_capacity(entries.len());
    for path in &entries {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
            let _ = fs::remove_file(path);
            continue;
        };
        events.push(value);
    }

    if events.is_empty() {
        return Ok(());
    }

    let url = std::env::var("BURROW_URL").unwrap_or_else(|_| "http://localhost:3001".into());

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(5))
        .build();

    match agent
        .post(&format!("{url}/api/events"))
        .send_json(serde_json::Value::Array(events))
    {
        Ok(_) => {
            for path in &entries {
                let _ = fs::remove_file(path);
            }
        }
        Err(_) => {}
    }

    Ok(())
}

fn post() -> anyhow::Result<()> {
    flush_events()
}

// ── CLI ──────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "pheromone", about = "Lightweight build observer")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Add source line to .zshrc so ~/.wezel/init.zsh is loaded.
    Init,
    /// Manage tool aliases in ~/.wezel/init.zsh.
    Alias {
        /// The tool to alias (e.g. cargo, go, npm).
        tool: String,
        /// Remove the alias instead of installing it.
        #[arg(long)]
        remove: bool,
    },
    Post {
        args: Vec<String>,
    },
    Pre {
        args: Vec<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => init(),
        Command::Alias { tool, remove } => {
            if remove {
                alias_remove(&tool)
            } else {
                alias_install(&tool)
            }
        }
        Command::Post { .. } => post(),
        Command::Pre { .. } => Ok(()),
    }
}
