use crate::{api, config, utils};
use anyhow::{Context, Result};
use std::process::{Command, Stdio};
use colored::Colorize;
use std::io::Write; 
use std::os::unix::fs::PermissionsExt;

/// 这是一个通用的 SSH 命令构建器，其他模块可以复用
pub async fn build_ssh_command(target_str: &str) -> Result<(Command, tempfile::NamedTempFile)> {
    let target = utils::parse_target(target_str)?;
    let full_domain = format!("{}.{}.ops.autos", target.environment, target.project);
    let ssh_target = format!("root@{}", full_domain);

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    println!("Fetching access credentials...");
    let key_resp = api::get_ci_private_key(&token, &target.project, &target.environment).await?;
    
    let mut temp_key_file = tempfile::NamedTempFile::new()?;
    writeln!(temp_key_file, "{}", key_resp.private_key)?;
    let meta = temp_key_file.as_file().metadata()?;
    let mut perms = meta.permissions();
    perms.set_mode(0o600);
    temp_key_file.as_file().set_permissions(perms)?;
    
    println!("{}", "✔ Access granted via CI Key.".green());
    let key_path = temp_key_file.path().to_str().unwrap();

    let mut cmd = Command::new("ssh");
    cmd.arg("-i").arg(key_path)
       .arg("-o").arg("StrictHostKeyChecking=no")
       .arg("-o").arg("UserKnownHostsFile=/dev/null")
       .arg(&ssh_target);

    Ok((cmd, temp_key_file))
}

// ops ssh <target> [command]
pub async fn handle_ssh(target_str: String, command: Option<String>) -> Result<()> {
    let (mut cmd, _temp_key_file) = build_ssh_command(&target_str).await?;

    if let Some(remote_cmd) = command {
        println!("Executing on {}...", target_str.cyan());
        cmd.arg(&remote_cmd);
        
        let mut child = cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit()).spawn()?;
        let status = child.wait()?;
        if !status.success() {
            return Err(anyhow::anyhow!("Remote command failed with status: {}", status));
        }
    } else {
        println!("Connecting...");
        let status = cmd.status().context("Failed to launch interactive ssh session")?;
        if !status.success() {
            // Interactive session errors are usually shown directly, but we can log here
        }
    }
    Ok(())
}

// 用于 env upload
pub async fn execute_remote_command(target_str: &str, command: &str, stdin_data: Option<&str>) -> Result<()> {
    let (mut cmd, _temp_key_file) = build_ssh_command(target_str).await?;
    cmd.arg(command);

    if let Some(data) = stdin_data {
        cmd.stdin(Stdio::piped());
        let mut child = cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit()).spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(data.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            return Err(anyhow::anyhow!("Remote command failed with status: {}", status));
        }
    } else {
        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow::anyhow!("Remote command failed with status: {}", status));
        }
    }
    Ok(())
}

// 用于 env download
pub async fn execute_remote_command_with_output(target_str: &str, command: &str) -> Result<Vec<u8>> {
    let (mut cmd, _temp_key_file) = build_ssh_command(target_str).await?;
    cmd.arg(command);

    let output = cmd.output().context("Failed to execute remote command and capture output")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Remote command failed with status: {}. Stderr: {}", output.status, stderr));
    }
    Ok(output.stdout)
}