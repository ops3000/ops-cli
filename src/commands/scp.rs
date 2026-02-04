// src/commands/scp.rs

use crate::{api, config, utils};
use crate::utils::TargetType;
use anyhow::{Context, Result};
use std::process::Command;
use colored::Colorize;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Push files to a target
/// Supports both Node ID (e.g., "12345:/root/") and App target (e.g., "api.RedQ:/root/")
pub async fn handle_push(source: String, target_str: String) -> Result<()> {
    // 1. 解析目标
    let target = utils::parse_target_v2(&target_str)?;
    let full_domain = target.domain();

    // 默认为 /root/，如果用户未指定路径
    let remote_path = target.path().map(|s| s.to_string()).unwrap_or_else(|| "/root/".to_string());
    let scp_destination = format!("root@{}:{}", full_domain, remote_path);

    println!("Pushing {} to {}...", source.cyan(), scp_destination.cyan());

    // 2. 获取凭证
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    println!("Fetching access credentials...");

    // Get CI key based on target type
    let private_key = match &target {
        TargetType::NodeId { id, .. } => {
            let key_resp = api::get_node_ci_key(&token, *id).await?;
            key_resp.private_key
        }
        TargetType::AppTarget { app, project, .. } => {
            let key_resp = api::get_app_ci_key(&token, project, app).await?;
            key_resp.private_key
        }
    };

    // 3. 准备私钥文件
    let mut temp_key_file = tempfile::NamedTempFile::new()?;
    writeln!(temp_key_file, "{}", private_key)?;

    let meta = temp_key_file.as_file().metadata()?;
    let mut perms = meta.permissions();
    perms.set_mode(0o600);
    temp_key_file.as_file().set_permissions(perms)?;

    let key_path = temp_key_file.path().to_str().unwrap();

    // 4. 执行 scp
    // scp -i key -o StrictHostKeyChecking=no -r source root@domain:path
    let mut cmd = Command::new("scp");
    cmd.arg("-i").arg(key_path)
       .arg("-o").arg("StrictHostKeyChecking=no")
       .arg("-o").arg("UserKnownHostsFile=/dev/null");

    // 如果源是目录，添加递归标志
    if Path::new(&source).is_dir() {
        cmd.arg("-r");
    }

    cmd.arg(&source)
       .arg(&scp_destination);

    let status = cmd.status().context("Failed to execute scp command")?;

    if status.success() {
        println!("{}", "✔ File transfer successful.".green());
    } else {
        return Err(anyhow::anyhow!("SCP command failed with status: {}", status));
    }

    Ok(())
}