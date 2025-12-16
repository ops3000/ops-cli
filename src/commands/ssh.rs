// src/commands/ssh.rs

use crate::utils;
use anyhow::{Context, Result};
use std::process::Command;
use colored::Colorize;

pub async fn handle_ssh(target_str: String) -> Result<()> {
    let target = utils::parse_target(&target_str)?;
    
    // 构造完整域名: main_server.jug0.ops.autos
    let full_domain = format!("{}.{}.ops.autos", target.environment, target.project);
    
    // 默认使用 root，这里可以扩展支持 user@env.project
    let ssh_target = format!("root@{}", full_domain);
    
    println!("Connecting to {}...", ssh_target.cyan());

    // 使用系统 ssh 命令接管当前进程
    // 使用 exec (Unix) 替换当前进程会更好，但在 Rust 中 Command::status 是跨平台最简单的
    let mut child = Command::new("ssh")
        .arg(&ssh_target)
        .spawn()
        .context("Failed to launch ssh command")?;

    let status = child.wait()?;

    if !status.success() {
        return Err(anyhow::anyhow!("SSH session ended with error"));
    }

    Ok(())
}