use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

const HOOK_MARKER: &str = "# >>> wezel pheromone >>>";
const HOOK_END: &str = "# <<< wezel pheromone <<<";
const FLUSH_LOCK: &str = ".flush.lock";

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

fn pheromone_dir() -> PathBuf {
    dirs::data_local_dir()
        .expect("could not determine local data directory")
        .join("pheromone")
}

fn events_dir() -> PathBuf {
    pheromone_dir().join("events")
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

/// Guard that removes the sentinel lock file on drop.
struct FlushLock {
    path: PathBuf,
}

impl FlushLock {
    /// Try to acquire the flush lock by atomically creating a sentinel file.
    /// Returns `None` if another process already holds it.
    fn try_acquire(dir: &std::path::Path) -> Option<Self> {
        let path = dir.join(FLUSH_LOCK);
        // create_new fails if the file already exists — atomic on the same filesystem.
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
        // Another process is already flushing.
        return Ok(());
    };

    // Snapshot the event files present right now.
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
            // Malformed file — remove it so it doesn't block future flushes.
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
        Err(_) => {
            // Server unreachable — keep events for next flush.
        }
    }

    // _lock dropped here, sentinel removed.
    Ok(())
}

fn post() -> anyhow::Result<()> {
    flush_events()
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => init(),
        Command::Post { .. } => post(),
        Command::Pre { .. } => anyhow::Ok(()),
        _ => anyhow::Ok(()),
    }
}
