use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use url::Url;

use crate::config;
use crate::pheromones_dir;

pub fn health_cmd() -> anyhow::Result<()> {
    // 1. List available pheromones
    let pdir = pheromones_dir();
    println!("pheromones dir: {}", pdir.display());
    if pdir.is_dir() {
        let mut found = false;
        for entry in fs::read_dir(&pdir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("pheromone-") {
                println!("  {name} ✓");
                found = true;
            }
        }
        if !found {
            println!("  (none found)");
        }
    } else {
        println!("  ⚠ directory not found");
    }

    // 2. Check global config
    println!();
    let global_path = config::global_config_path();
    if global_path.is_file() {
        println!("global config: {} ✓", global_path.display());
    } else {
        println!("global config: {} (not found)", global_path.display());
    }

    // 3. Check project config
    let cwd = std::env::current_dir().unwrap_or_default();
    println!();
    match config::discover(&cwd) {
        Some((wezel_dir, config)) => {
            println!(
                "project config: {} ✓",
                wezel_dir.join("config.toml").display()
            );
            println!("  burrow_url: {}", config.burrow_url);
            println!("  username: {}", config.username);

            // 4. Ping burrow
            println!();
            print!("burrow ({}): ", config.burrow_url);
            match ping_burrow(&config.burrow_url) {
                Ok(()) => println!("reachable ✓"),
                Err(e) => println!("⚠ unreachable — {e}"),
            }
        }
        None => {
            println!("project config: ⚠ no .wezel/config.toml found (run `wezel setup`)");
        }
    }

    Ok(())
}

fn ping_burrow(base: &str) -> anyhow::Result<()> {
    let url = Url::parse(base)?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("no host in URL"))?;
    let port = url.port_or_known_default().unwrap_or(80);

    let addr = format!("{host}:{port}");
    let resolved = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("DNS resolution failed for {addr}"))?;

    TcpStream::connect_timeout(&resolved, Duration::from_secs(3))?;
    Ok(())
}
