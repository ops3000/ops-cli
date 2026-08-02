use crate::{api, config, utils};
use crate::utils::Target;
use anyhow::{Context, Result};
use std::process::{Command, Stdio};
use colored::Colorize;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;

/// SSH 连接方式: 智能路由的解析结果
enum SshRoute {
    /// 直连 host (公网域名或局域网 IP)
    Direct(String),
    /// 经 Cloudflare Tunnel, 用 cloudflared access 做 ProxyCommand
    Tunnel(String),
}

impl SshRoute {
    fn host(&self) -> &str {
        match self {
            SshRoute::Direct(h) | SshRoute::Tunnel(h) => h,
        }
    }

    /// 把路由应用到 ssh Command (Tunnel 时附加 ProxyCommand)
    fn apply(&self, cmd: &mut Command) {
        if let SshRoute::Tunnel(hostname) = self {
            cmd.arg("-o").arg(format!(
                "ProxyCommand=cloudflared access ssh --hostname {}",
                hostname
            ));
        }
    }
}

/// 400ms 内能建立 TCP 连接就认为可直连
fn tcp_probe(ip: &str, port: u16) -> bool {
    use std::net::ToSocketAddrs;
    let addr = match format!("{}:{}", ip, port).to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(a) => a,
            None => return false,
        },
        Err(_) => return false,
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(400)).is_ok()
}

fn cloudflared_available() -> bool {
    std::process::Command::new("which")
        .arg("cloudflared")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Node 目标的智能路由: 局域网直连 → Cloudflare Tunnel → 公网域名。
/// 任何一步失败都回落到公网域名, 保证行为不比从前差。
async fn resolve_node_route(token: &str, node_id: u64, fallback_domain: &str) -> SshRoute {
    let node = match api::get_node(token, node_id).await {
        Ok(n) => n,
        Err(_) => return SshRoute::Direct(fallback_domain.to_string()),
    };

    // 1. 同一局域网内直连 (心跳上报的内网 IP, 400ms 探测)
    if let Some(lan_ip) = &node.lan_ip {
        if tcp_probe(lan_ip, 22) {
            o_debug!("{}", format!("Using LAN direct connection ({})", lan_ip).green());
            return SshRoute::Direct(lan_ip.clone());
        }
    }

    // 2. Cloudflare Tunnel (无公网 IP 的节点)
    if node.has_ssh_tunnel != 0 {
        if let Some(tunnel_domain) = &node.ssh_tunnel_domain {
            if cloudflared_available() {
                o_debug!("Using Cloudflare Tunnel ({})", tunnel_domain);
                return SshRoute::Tunnel(tunnel_domain.clone());
            }
            o_warn!(
                "{}",
                "This node uses a Cloudflare Tunnel but `cloudflared` is not installed.\n  Install it: brew install cloudflared (macOS) / https://developers.cloudflare.com/cloudflared/".yellow()
            );
        }
    }

    // 3. 公网域名
    SshRoute::Direct(fallback_domain.to_string())
}

/// 供 scp 等其他模块复用: 解析目标的最佳连接方式。
/// 返回 (host, 可选的 "ProxyCommand=..." 完整选项)。
pub async fn resolve_target_route(token: &str, target: &Target) -> (String, Option<String>) {
    let full_domain = target.domain();
    match target {
        Target::NodeId { id, .. } => {
            let route = resolve_node_route(token, *id, &full_domain).await;
            let proxy = match &route {
                SshRoute::Tunnel(hostname) => Some(format!(
                    "ProxyCommand=cloudflared access ssh --hostname {}",
                    hostname
                )),
                SshRoute::Direct(_) => None,
            };
            (route.host().to_string(), proxy)
        }
        Target::AppTarget { .. } => (full_domain, None),
    }
}

/// 这是一个通用的 SSH 命令构建器，其他模块可以复用
/// Supports both Node ID (e.g., "12345") and App target (e.g., "api.RedQ")
pub async fn build_ssh_command(target_str: &str) -> Result<(Command, tempfile::NamedTempFile)> {
    let target = utils::parse_target(target_str)?;
    let full_domain = target.domain();

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    o_debug!("Fetching access credentials...");

    // Get CI key based on target type; node targets also resolve the best route
    let (private_key, route) = match &target {
        Target::NodeId { id, .. } => {
            let key_resp = api::get_node_ci_key(&token, *id).await?;
            let route = resolve_node_route(&token, *id, &full_domain).await;
            (key_resp.private_key, route)
        }
        Target::AppTarget { app, project, .. } => {
            let key_resp = api::get_app_ci_key(&token, project, app).await?;
            (key_resp.private_key, SshRoute::Direct(full_domain.clone()))
        }
    };
    let ssh_target = format!("root@{}", route.host());

    let mut temp_key_file = tempfile::NamedTempFile::new()?;
    writeln!(temp_key_file, "{}", private_key)?;
    let meta = temp_key_file.as_file().metadata()?;
    let mut perms = meta.permissions();
    perms.set_mode(0o600);
    temp_key_file.as_file().set_permissions(perms)?;

    o_debug!("{}", "✔ Access granted via CI Key.".green());
    let key_path = temp_key_file.path().to_str().unwrap();

    let mut cmd = Command::new("ssh");
    cmd.arg("-i").arg(key_path)
       .arg("-o").arg("StrictHostKeyChecking=no")
       .arg("-o").arg("UserKnownHostsFile=/dev/null")
       .arg("-o").arg("LogLevel=ERROR");
    route.apply(&mut cmd);
    cmd.arg(&ssh_target);

    Ok((cmd, temp_key_file))
}

/// 可复用的 SSH 会话，一次 fetch CI key，多次执行命令
pub struct SshSession {
    ssh_target: String,
    _temp_key_file: tempfile::NamedTempFile,
    key_path: String,
    target_str: String,
    /// Tunnel 路由时的 "ProxyCommand=..." 完整选项, ssh 和 rsync -e 都要带上
    proxy_command: Option<String>,
}

impl SshSession {
    /// 建立会话：fetch CI key，创建 temp key file（只做一次）
    pub async fn connect(target_str: &str) -> Result<Self> {
        let target = utils::parse_target(target_str)?;
        let full_domain = target.domain();

        let cfg = config::load_config().context("Config error")?;
        let token = cfg.token.context("Please run `ops login` first.")?;

        o_debug!("Fetching access credentials...");

        let (private_key, route) = match &target {
            Target::NodeId { id, .. } => {
                let key_resp = api::get_node_ci_key(&token, *id).await?;
                let route = resolve_node_route(&token, *id, &full_domain).await;
                (key_resp.private_key, route)
            }
            Target::AppTarget { app, project, .. } => {
                let key_resp = api::get_app_ci_key(&token, project, app).await?;
                (key_resp.private_key, SshRoute::Direct(full_domain.clone()))
            }
        };
        let ssh_target = format!("root@{}", route.host());
        let proxy_command = match &route {
            SshRoute::Tunnel(hostname) => Some(format!(
                "ProxyCommand=cloudflared access ssh --hostname {}",
                hostname
            )),
            SshRoute::Direct(_) => None,
        };

        let mut temp_key_file = tempfile::NamedTempFile::new()?;
        writeln!(temp_key_file, "{}", private_key)?;
        let meta = temp_key_file.as_file().metadata()?;
        let mut perms = meta.permissions();
        perms.set_mode(0o600);
        temp_key_file.as_file().set_permissions(perms)?;

        let key_path = temp_key_file.path().to_str().unwrap().to_string();

        o_debug!("{}", "✔ Access granted via CI Key.".green());

        Ok(Self { ssh_target, _temp_key_file: temp_key_file, key_path, target_str: target_str.to_string(), proxy_command })
    }

    /// 返回原始 target 标识符（如 "4" 或 "api.RedQ"），供 scp/rsync 使用
    pub fn target(&self) -> &str {
        &self.target_str
    }

    /// 构建 ssh Command，复用已有的 key
    fn command(&self) -> Command {
        let mut cmd = Command::new("ssh");
        cmd.arg("-i").arg(&self.key_path)
           .arg("-o").arg("StrictHostKeyChecking=no")
           .arg("-o").arg("UserKnownHostsFile=/dev/null")
           .arg("-o").arg("LogLevel=ERROR");
        if let Some(pc) = &self.proxy_command {
            cmd.arg("-o").arg(pc);
        }
        cmd.arg(&self.ssh_target);
        cmd
    }

    /// 执行远程命令（stdout/stderr 直接输出）
    pub fn exec(&self, command: &str, stdin_data: Option<&str>) -> Result<()> {
        let mut cmd = self.command();
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

    /// rsync 本地目录到远程，复用已有的 key
    /// `include` 为白名单：非空时只同步列出的路径，其余排除
    /// 支持 `..` 开头的路径（项目目录外的依赖），会单独 rsync 到远程对应子目录
    pub fn rsync_push(&self, remote_path: &str, include: &[String]) -> Result<()> {
        // rsync 的 -e 字符串按空格分词但支持引号, ProxyCommand 的值必须整体加引号
        let proxy_part = self.proxy_command.as_deref()
            .map(|pc| format!(" -o \"{}\"", pc))
            .unwrap_or_default();
        let ssh_cmd = format!(
            "ssh -i {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR{}",
            self.key_path, proxy_part
        );
        let remote = format!("{}:{}/", self.ssh_target, remote_path);

        // Separate entries: parent-relative (../) vs local
        let (external, local): (Vec<_>, Vec<_>) = include.iter()
            .partition(|e| e.starts_with("../"));

        // 1. Sync local entries with include/exclude filters
        {
            let mut cmd = Command::new("rsync");
            cmd.arg("-az")
                .arg("--progress")
                .arg("--delete")
                .arg("-e").arg(&ssh_cmd)
                .arg("--exclude").arg("target/")
                .arg("--exclude").arg("node_modules/")
                .arg("--exclude").arg(".git/")
                .arg("--exclude").arg(".env");

            if !local.is_empty() {
                for entry in &local {
                    let pattern = if entry.contains('.') && !entry.ends_with('/') {
                        format!("/{}", entry)
                    } else {
                        let trimmed = entry.trim_end_matches('/');
                        format!("/{}/***", trimmed)
                    };
                    cmd.arg("--include").arg(pattern);
                }
                cmd.arg("--exclude").arg("*");
            }

            cmd.arg("./").arg(&remote);

            let status = cmd.status()
                .context("Failed to execute rsync (is rsync installed?)")?;
            if !status.success() {
                return Err(anyhow::anyhow!("rsync failed with status: {}", status));
            }
        }

        // 2. Sync external (../) entries individually
        for entry in &external {
            // e.g. "../juglans/jug0" → local source: "../juglans/jug0/", remote dest: "<remote_path>/jug0/"
            let dir_name = std::path::Path::new(entry.as_str())
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| entry.trim_end_matches('/').to_string());

            let src = format!("{}/", entry.trim_end_matches('/'));
            let dst = format!("{}:{}/{}/", self.ssh_target, remote_path, dir_name);

            let mut cmd = Command::new("rsync");
            cmd.arg("-az")
                .arg("--progress")
                .arg("--delete")
                .arg("-e").arg(&ssh_cmd)
                .arg("--exclude").arg("target/")
                .arg("--exclude").arg("node_modules/")
                .arg("--exclude").arg(".git/")
                .arg("--exclude").arg(".env")
                .arg(&src)
                .arg(&dst);

            let status = cmd.status()
                .context(format!("Failed to rsync external path: {}", entry))?;
            if !status.success() {
                return Err(anyhow::anyhow!("rsync failed for '{}' with status: {}", entry, status));
            }
        }

        Ok(())
    }

    /// 执行远程命令并捕获 stdout
    pub fn exec_output(&self, command: &str) -> Result<Vec<u8>> {
        let mut cmd = self.command();
        cmd.arg(command);

        let output = cmd.output().context("Failed to execute remote command")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Remote command failed: {}. {}", output.status, stderr));
        }
        Ok(output.stdout)
    }
}

// ops ssh <target> [command]
pub async fn handle_ssh(target_str: String, command: Option<String>) -> Result<()> {
    let (mut cmd, _temp_key_file) = build_ssh_command(&target_str).await?;

    if let Some(remote_cmd) = command {
        o_step!("Executing on {}...", target_str.cyan());
        cmd.arg(&remote_cmd);

        let mut child = cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit()).spawn()?;
        let status = child.wait()?;
        if !status.success() {
            return Err(anyhow::anyhow!("Remote command failed with status: {}", status));
        }
    } else {
        o_debug!("Connecting...");
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