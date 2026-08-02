// src/serve/tunnel.rs
// cloudflared 子进程管理: 无公网 IP 节点的 SSH 隧道。
// 心跳发现 tunnel_enabled 后, 确保 cloudflared 二进制存在 (缺失时自动下载),
// 用 serve token 换取 tunnel token 并保持 `cloudflared tunnel run` 运行。

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::PathBuf;
use tokio::process::{Child, Command};

const INSTALL_PATH: &str = "/usr/local/bin/cloudflared";

/// 心跳每 60 秒调一次: 进程还活着就直接返回, 退出了或没启动就 (重新) 拉起。
/// 失败只打日志不返回错误 —— 下一次心跳自然重试, 60s 间隔就是退避。
pub async fn ensure_running(child_slot: &mut Option<Child>, serve_token: &str, node_id: u64) {
    if let Some(child) = child_slot.as_mut() {
        match child.try_wait() {
            Ok(None) => return, // still running
            _ => {
                *child_slot = None;
                eprintln!("{}", "cloudflared exited, restarting...".yellow());
            }
        }
    }

    match start(serve_token, node_id).await {
        Ok(child) => *child_slot = Some(child),
        Err(e) => eprintln!("{}", format!("⚠ cloudflared start failed: {}", e).yellow()),
    }
}

pub async fn stop(child_slot: &mut Option<Child>) {
    if let Some(mut child) = child_slot.take() {
        let _ = child.kill().await;
    }
}

async fn start(serve_token: &str, node_id: u64) -> Result<Child> {
    let bin = ensure_binary().await?;
    let resp = crate::api::get_tunnel_token(serve_token, node_id).await?;

    let child = Command::new(&bin)
        .args(["tunnel", "run", "--token", &resp.token])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("Failed to spawn {}", bin.display()))?;

    eprintln!(
        "{}",
        format!("✓ cloudflared tunnel running ({})", resp.ssh_tunnel_domain).green()
    );
    Ok(child)
}

/// PATH 里找 cloudflared, 找不到就下载官方静态二进制到 /usr/local/bin
async fn ensure_binary() -> Result<PathBuf> {
    if let Ok(output) = std::process::Command::new("which").arg("cloudflared").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    let install = PathBuf::from(INSTALL_PATH);
    if install.exists() {
        return Ok(install);
    }

    if std::env::consts::OS != "linux" {
        anyhow::bail!(
            "cloudflared not found. Install it manually: https://developers.cloudflare.com/cloudflared/"
        );
    }
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => anyhow::bail!("Unsupported architecture for cloudflared auto-install: {}", other),
    };

    let url = format!(
        "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-{}",
        arch
    );
    eprintln!("{}", "Downloading cloudflared...".cyan());
    let bytes = reqwest::get(&url)
        .await?
        .error_for_status()
        .context("cloudflared download failed")?
        .bytes()
        .await?;

    tokio::fs::write(&install, &bytes)
        .await
        .with_context(|| format!("Failed to write {} (are we root?)", INSTALL_PATH))?;
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(&install, std::fs::Permissions::from_mode(0o755)).await?;

    eprintln!("{}", format!("✓ cloudflared installed to {}", INSTALL_PATH).green());
    Ok(install)
}
