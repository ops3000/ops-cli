use crate::{api, config, utils};
use crate::utils::Target;
use anyhow::{Context, Result};
use std::process::{Command, Stdio};
use colored::Colorize;
use std::io::Write;

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

/// 对面是不是真的有个 sshd 在等我们 —— 连上并读到 SSH 协议横幅才算数。
///
/// 判据不是「TCP 能建立」, 而且这个区别是这个函数存在的全部理由。一台开着
/// 全局 VPN / TUN 代理的机器 (Clash, sing-box, Karing, WARP…) 会把默认路由
/// 指向隧道, 而隧道的用户态栈是**先接受本地连接, 再去连后端** —— 于是
/// `connect_timeout` 对任何 IP 任何端口都立刻成功, 包括根本没人监听的端口。
/// 拿它当"可直连"的证据, 等于问一个总是答"是"的证人。
///
/// 真实后果 (2026-08-20): 一台 GCP 机器的 lan_ip 是它的 VPC 内网地址
/// `10.140.0.2`, CLI 所在的 Mac 显然不在那个 VPC 里, 但 Mac 开着全局代理,
/// 探测返回 true, 于是 `ops ssh` 一头撞进死路, 报
/// `kex_exchange_identification: read: Connection reset by peer` —— 一个
/// 指向 SSH 握手的错误, 而真正的问题是地址根本不通。公网那条路明明是好的。
/// 这类云 (GCP / AWS / 阿里云…) 全都中招: 实例自己只看得见内网地址, 公网 IP
/// 在 NAT 后面, 所以 `ops init` 一定会把 VPC 内网地址记成 lan_ip。
///
/// 读横幅额外挡住的:端口被别的服务占着 (那也不该走 SSH)、中间有个 TCP 层
/// 负载均衡接了连接但后端是空的。两种情况下回落公网都是对的。
///
/// 预算 ~800ms 最坏情况 (连接 400 + 读 400)。局域网里的 sshd 连上就发横幅,
/// 通常几毫秒;慢到读不着, 那也不配叫"直连"。
fn ssh_probe(ip: &str, port: u16) -> bool {
    use std::io::Read;
    use std::net::ToSocketAddrs;

    let addr = match format!("{}:{}", ip, port).to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(a) => a,
            None => return false,
        },
        Err(_) => return false,
    };
    let timeout = std::time::Duration::from_millis(400);
    let mut stream = match std::net::TcpStream::connect_timeout(&addr, timeout) {
        Ok(s) => s,
        Err(_) => return false,
    };
    if stream.set_read_timeout(Some(timeout)).is_err() {
        return false;
    }
    // "SSH-" 就够断言了: 协议规定服务端一连上就先发 `SSH-<版本>-<软件>`。
    let mut buf = [0u8; 4];
    let mut got = 0;
    while got < buf.len() {
        match stream.read(&mut buf[got..]) {
            Ok(0) => return false, // 对端直接关了 —— 隧道连不上后端时就是这样
            Ok(n) => got += n,
            Err(_) => return false, // 读超时 = 没人说话
        }
    }
    &buf == b"SSH-"
}

fn rsync_available() -> bool {
    std::process::Command::new("rsync")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 直接跑 `cloudflared --version` 探测, 跨平台 (Windows 没有 which)
fn cloudflared_available() -> bool {
    std::process::Command::new("cloudflared")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Node 目标的智能路由: 局域网直连 → Cloudflare Tunnel → 公网域名。
/// 任何一步失败都回落到公网域名, 保证行为不比从前差。
/// 同时带回节点配置的 SSH 登录用户 (Windows 节点注册时自动上报, None = root)。
async fn resolve_node_route(token: &str, node_id: u64, fallback_domain: &str) -> (SshRoute, Option<String>) {
    let node = match api::get_node(token, node_id).await {
        Ok(n) => n,
        Err(_) => return (SshRoute::Direct(fallback_domain.to_string()), None),
    };

    let ssh_user = node.ssh_user.clone();

    // 1. 同一局域网内直连 (心跳上报的内网 IP)。探测要求读到 SSH 横幅, 不是
    //    只要 TCP 能连上 —— 见 `ssh_probe`: 全局 VPN 会让后者对任何地址都成立。
    if let Some(lan_ip) = &node.lan_ip {
        if ssh_probe(lan_ip, 22) {
            o_debug!("{}", format!("Using LAN direct connection ({})", lan_ip).green());
            return (SshRoute::Direct(lan_ip.clone()), ssh_user);
        }
    }

    // 2. Cloudflare Tunnel (无公网 IP 的节点)
    if node.has_ssh_tunnel != 0 {
        if let Some(tunnel_domain) = &node.ssh_tunnel_domain {
            if cloudflared_available() {
                o_debug!("Using Cloudflare Tunnel ({})", tunnel_domain);
                return (SshRoute::Tunnel(tunnel_domain.clone()), ssh_user);
            }
            o_warn!(
                "{}",
                "This node uses a Cloudflare Tunnel but `cloudflared` is not installed.\n  Install it: brew install cloudflared (macOS) / https://developers.cloudflare.com/cloudflared/".yellow()
            );
        }
    }

    // 3. 公网域名
    (SshRoute::Direct(fallback_domain.to_string()), ssh_user)
}

/// 供 scp 等其他模块复用: 解析目标的最佳连接方式。
/// 返回 (host, 可选的 "ProxyCommand=..." 完整选项, 节点配置的 SSH 用户)。
pub async fn resolve_target_route(token: &str, target: &Target) -> (String, Option<String>, Option<String>) {
    let full_domain = target.domain();
    match target {
        Target::NodeId { id, .. } => {
            let (route, ssh_user) = resolve_node_route(token, *id, &full_domain).await;
            let proxy = match &route {
                SshRoute::Tunnel(hostname) => Some(format!(
                    "ProxyCommand=cloudflared access ssh --hostname {}",
                    hostname
                )),
                SshRoute::Direct(_) => None,
            };
            (route.host().to_string(), proxy, ssh_user)
        }
        Target::AppTarget { .. } => (full_domain, None, None),
    }
}

/// 这是一个通用的 SSH 命令构建器，其他模块可以复用
/// Supports both Node ID (e.g., "12345") and App target (e.g., "api.RedQ")
/// `user`: SSH 登录用户, 默认 root (Windows 节点没有 root, 用 -l 指定)
pub async fn build_ssh_command(target_str: &str, user: Option<&str>) -> Result<(Command, tempfile::NamedTempFile)> {
    let target = utils::parse_target(target_str)?;
    let full_domain = target.domain();

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    o_debug!("Fetching access credentials...");

    // Get CI key based on target type; node targets also resolve the best route
    let (private_key, route, node_user) = match &target {
        Target::NodeId { id, .. } => {
            let key_resp = api::get_node_ci_key(&token, *id).await?;
            let (route, ssh_user) = resolve_node_route(&token, *id, &full_domain).await;
            (key_resp.private_key, route, ssh_user)
        }
        Target::AppTarget { app, project, .. } => {
            let key_resp = api::get_app_ci_key(&token, project, app).await?;
            (key_resp.private_key, SshRoute::Direct(full_domain.clone()), None)
        }
    };
    // 优先级: -l 参数 > 节点配置的 ssh_user > root
    let login = user.map(str::to_string)
        .or(node_user)
        .unwrap_or_else(|| "root".to_string());
    let ssh_target = format!("{}@{}", login, route.host());

    let mut temp_key_file = tempfile::NamedTempFile::new()?;
    writeln!(temp_key_file, "{}", private_key)?;
    utils::secure_key_permissions(temp_key_file.as_file())?;

    o_debug!("{}", "✔ Access granted via CI Key.".green());
    let key_path = temp_key_file.path().to_str().unwrap();

    let mut cmd = Command::new("ssh");
    cmd.arg("-i").arg(key_path)
       .arg("-o").arg("StrictHostKeyChecking=no")
       .arg("-o").arg(utils::SSH_KNOWN_HOSTS_OPT)
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

        let (private_key, route, node_user) = match &target {
            Target::NodeId { id, .. } => {
                let key_resp = api::get_node_ci_key(&token, *id).await?;
                let (route, ssh_user) = resolve_node_route(&token, *id, &full_domain).await;
                (key_resp.private_key, route, ssh_user)
            }
            Target::AppTarget { app, project, .. } => {
                let key_resp = api::get_app_ci_key(&token, project, app).await?;
                (key_resp.private_key, SshRoute::Direct(full_domain.clone()), None)
            }
        };
        let ssh_target = format!("{}@{}", node_user.as_deref().unwrap_or("root"), route.host());
        let proxy_command = match &route {
            SshRoute::Tunnel(hostname) => Some(format!(
                "ProxyCommand=cloudflared access ssh --hostname {}",
                hostname
            )),
            SshRoute::Direct(_) => None,
        };

        let mut temp_key_file = tempfile::NamedTempFile::new()?;
        writeln!(temp_key_file, "{}", private_key)?;
        utils::secure_key_permissions(temp_key_file.as_file())?;

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
           .arg("-o").arg(utils::SSH_KNOWN_HOSTS_OPT)
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
        // Windows 原生没有 rsync, 提前给出明确提示而不是让命令启动失败
        if cfg!(windows) && !rsync_available() {
            anyhow::bail!(
                "rsync not found. `ops push` / push-mode deploy requires rsync.\n  \
                 Install it (e.g. `scoop install rsync` or MSYS2/cwRsync), or use WSL."
            );
        }
        // rsync 的 -e 字符串按空格分词但支持引号, ProxyCommand 的值必须整体加引号;
        // key 路径也加引号 (Windows 临时目录可能含空格)
        let proxy_part = self.proxy_command.as_deref()
            .map(|pc| format!(" -o \"{}\"", pc))
            .unwrap_or_default();
        let ssh_cmd = format!(
            "ssh -i \"{}\" -o StrictHostKeyChecking=no -o {} -o LogLevel=ERROR{}",
            self.key_path, utils::SSH_KNOWN_HOSTS_OPT, proxy_part
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
pub async fn handle_ssh(target_str: String, command: Option<String>, user: Option<&str>) -> Result<()> {
    let (mut cmd, _temp_key_file) = build_ssh_command(&target_str, user).await?;

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
    let (mut cmd, _temp_key_file) = build_ssh_command(target_str, None).await?;
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
    let (mut cmd, _temp_key_file) = build_ssh_command(target_str, None).await?;
    cmd.arg(command);

    let output = cmd.output().context("Failed to execute remote command and capture output")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Remote command failed with status: {}. Stderr: {}", output.status, stderr));
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    /// 起一个假服务器: 接受一条连接, 按 `speak` 决定说什么, 然后关闭。
    /// 返回它的端口。
    fn fake_server(speak: Option<&'static [u8]>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                if let Some(bytes) = speak {
                    let _ = sock.write_all(bytes);
                    let _ = sock.flush();
                    // 让对端有机会读完再关。
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                // speak = None ⇒ 立刻 drop, 这正是隧道连不上后端时的行为。
            }
        });
        port
    }

    /// 真的 sshd: 连上就发横幅。
    #[test]
    fn a_real_sshd_is_reachable() {
        let port = fake_server(Some(b"SSH-2.0-OpenSSH_9.6p1 Ubuntu-3ubuntu13.16\r\n"));
        assert!(ssh_probe("127.0.0.1", port));
    }

    /// THE BUG. 一个接受 TCP 然后一言不发就关掉的对端 —— 全局 VPN / TUN 代理
    /// 对它够不着的地址就是这个表现。旧的 `tcp_probe` 在这里返回 true, 于是
    /// `ops ssh` 选了一条走不通的路, 并用一个 SSH 握手错误来报告一个路由问题。
    #[test]
    fn a_tunnel_that_accepts_and_hangs_up_is_not_reachable() {
        let port = fake_server(None);
        assert!(
            !ssh_probe("127.0.0.1", port),
            "accepting a TCP connection is not evidence that anything is listening"
        );
    }

    /// 端口被别的服务占着 (HTTP / 数据库 / 随便什么) 也不是 SSH。
    #[test]
    fn some_other_service_on_the_port_is_not_ssh() {
        let port = fake_server(Some(b"HTTP/1.1 400 Bad Request\r\n\r\n"));
        assert!(!ssh_probe("127.0.0.1", port));
    }

    /// 没人监听 ⇒ 连不上 ⇒ false。(注意这条在开着全局 TUN 的机器上可能失败,
    /// 因为隧道会接受这个连接 —— 但那正是上面第二条测的东西, 而且 127.0.0.1
    /// 通常不走隧道。)
    #[test]
    fn a_closed_port_is_not_reachable() {
        // 绑了就丢, 端口随即空出来。
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").expect("bind");
            l.local_addr().expect("addr").port()
        };
        assert!(!ssh_probe("127.0.0.1", port));
    }
}