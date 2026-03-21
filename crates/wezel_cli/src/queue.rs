//! Local event queue at `~/.wezel/queue/`.
//!
//! Pheromone-wrapped builds write `BuildEvent` JSON files here.
//! The daemon (or `wezel sync`) flushes them to burrow.

use std::path::{Path, PathBuf};

use uuid::Uuid;
use wezel_types::BuildEvent;

pub fn queue_dir() -> PathBuf {
    crate::wezel_dir().join("queue")
}

/// Write a `BuildEvent` to the queue directory as `<tool>-<uuid>.json`.
/// Returns the written path on success.
#[expect(unused)]
pub fn enqueue(tool: &str, event: &BuildEvent) -> std::io::Result<PathBuf> {
    let dir = queue_dir();
    std::fs::create_dir_all(&dir)?;
    let id = Uuid::new_v4();
    let path = dir.join(format!("{tool}-{id}.json"));
    let json = serde_json::to_string(event).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Read all queued events from `dir`. Returns `(path, event)` pairs.
/// Silently skips files that fail to parse.
pub fn read_all(dir: &Path) -> Vec<(PathBuf, BuildEvent)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&path)
            && let Ok(event) = serde_json::from_str::<BuildEvent>(&content)
        {
            out.push((path, event));
        }
    }
    out
}

/// Flush all queued events to burrow. Deletes successfully sent files.
/// Returns the number of events successfully flushed.
pub fn flush_queue(server_url: &str) -> usize {
    let dir = queue_dir();
    let events = read_all(&dir);
    if events.is_empty() {
        return 0;
    }

    let agent = ureq::Agent::new();
    let url = format!("{}/api/events", server_url.trim_end_matches('/'));
    let mut flushed = 0;

    for (path, event) in events {
        let payload = match serde_json::to_value(&event) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let body = serde_json::json!([payload]);
        match agent.post(&url).send_json(&body) {
            Ok(_) => {
                let _ = std::fs::remove_file(&path);
                flushed += 1;
            }
            Err(e) => {
                log::warn!("queue: failed to flush {}: {e}", path.display());
            }
        }
    }
    flushed
}
