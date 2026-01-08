// src/commands/env.rs
use crate::{config, ssh, utils};
use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::process::Command;

// ops env upload <target>
pub async fn handle_upload(target_str: String) -> Result<()> {
    let local_env_path = "./.env";
    if !fs::metadata(local_env_path).is_ok() {
        return Err(anyhow::anyhow!("Local file './.env' not found."));
    }
    
    let content = fs::read_to_string(local_env_path)
        .context("Failed to read local .env file")?;

    println!("Uploading local .env to {}...", target_str.cyan());
    
    // 远程路径固定
    let remote_path = format!("/opt/judge/.env");
    let command = format!("sudo tee {}", remote_path);

    // 使用 ssh 模块的通用函数执行
    ssh::execute_remote_command(&target_str, &command, Some(&content)).await?;

    println!("{}", "✔ .env file uploaded successfully.".green());
    Ok(())
}

// ops env download <target>
pub async fn handle_download(target_str: String) -> Result<()> {
    println!("Downloading .env from {}...", target_str.cyan());
    
    let remote_path = format!("/opt/judge/.env");
    let command = format!("sudo cat {}", remote_path);

    // 使用 ssh 模块的通用函数执行并捕获输出
    let output = ssh::execute_remote_command_with_output(&target_str, &command).await?;
    
    fs::write("./.env", &output).context("Failed to write to local .env file")?;

    println!("{}", "✔ .env file downloaded successfully.".green());
    Ok(())
}