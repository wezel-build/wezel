use std::fs;
use std::path::PathBuf;

use crate::wezel_dir;

const FLUSH_LOCK: &str = ".flush.lock";

fn events_dir() -> PathBuf {
    wezel_dir().join("events")
}

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

pub fn flush_events() -> anyhow::Result<()> {
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
