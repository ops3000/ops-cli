// src/commands/ping.rs

use crate::utils;
use anyhow::{Context, Result};
use colored::Colorize;
use std::process::Command;

pub async fn handle_ping(target_str: String) -> Result<()> {
    let target = utils::parse_target(&target_str)?;
    let full_domain = format!("{}.{}.ops.autos", target.environment, target.project);

    println!("Pinging {}...", full_domain.cyan());

    // 在不同操作系统上，ping 命令的参数可能略有不同
    // 但通常直接 ping 域名是通用的
    // 我们使用 spawn 而不是 status，这样用户可以看到实时的 ping 输出
    let mut child = Command::new("ping")
        .arg(&full_domain)
        .spawn()
        .context("Failed to execute 'ping' command. Is it installed and in your PATH?")?;

    let status = child.wait()?;

    if !status.success() {
        return Err(anyhow::anyhow!("Ping command finished with an error. The host may be unreachable."));
    }

    Ok(())
}