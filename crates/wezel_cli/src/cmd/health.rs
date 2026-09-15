use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use url::Url;

use crate::config;
use crate::pheromones_dir;
use crate::style;

pub fn health_cmd() -> anyhow::Result<()> {
    // 1. List available pheromones
    let pdir = pheromones_dir();
    println!("pheromones dir: {}", style::muted(pdir.display()));
    if pdir.is_dir() {
        let mut found = false;
        for entry in fs::read_dir(&pdir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("pheromone-") {
                println!("  {} {}", style::strong(name), style::success("✓"));
                found = true;
            }
        }
        if !found {
            println!("  {}", style::muted("(none found)"));
        }
    } else {
        println!("  {}", style::warning("⚠ directory not found"));
    }

    // 2. Check global config
    println!();
    let global_path = config::global_config_path();
    if global_path.is_file() {
        println!(
            "global config: {} {}",
            style::muted(global_path.display()),
            style::success("✓")
        );
    } else {
        println!(
            "global config: {} {}",
            style::muted(global_path.display()),
            style::warning("(not found)")
        );
    }

    // 3. Check project config
    let cwd = std::env::current_dir().unwrap_or_default();
    println!();
    match config::discover(&cwd) {
        Some((wezel_dir, config)) => {
            println!(
                "project config: {} {}",
                style::muted(wezel_dir.join("config.toml").display()),
                style::success("✓")
            );
            match &config.server_url {
                Some(url) => println!("  WEZEL_API_URL: {}", style::strong(url)),
                None => println!("  WEZEL_API_URL: {}", style::warning("(not set)")),
            }
            println!("  username: {}", style::strong(&config.username));

            // 4. Ping server
            if let Some(ref url) = config.server_url {
                println!();
                print!("server ({url}): ");
                match ping_fiflok(url) {
                    Ok(()) => println!("{}", style::success("reachable ✓")),
                    Err(e) => println!("{}", style::warning(format!("⚠ unreachable — {e}"))),
                }
            }
        }
        None => {
            println!(
                "project config: {}",
                style::warning("⚠ no .wezel/config.toml found (run `wezel project init`)")
            );
        }
    }

    Ok(())
}

fn ping_fiflok(base: &str) -> anyhow::Result<()> {
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
