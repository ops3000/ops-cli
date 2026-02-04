// src/commands/update.rs

use crate::update;
use anyhow::Result;
use colored::Colorize;

pub async fn handle_update() -> Result<()> {
    // self_update 是同步阻塞的，必须在 spawn_blocking 中运行
    tokio::task::spawn_blocking(|| {
        update::update_self()
    }).await??;

    // Restart ops-serve if it's running
    let status = std::process::Command::new("systemctl")
        .args(["is-active", "ops-serve"])
        .output();

    if let Ok(output) = status {
        if output.status.success() {
            println!("{}", "Restarting ops-serve...".yellow());
            let restart = std::process::Command::new("systemctl")
                .args(["restart", "ops-serve"])
                .status();

            if let Ok(s) = restart {
                if s.success() {
                    println!("{}", "✔ ops-serve restarted".green());
                }
            }
        }
    }

    Ok(())
}