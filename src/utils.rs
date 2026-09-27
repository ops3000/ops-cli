// src/utils.rs

use anyhow::{anyhow, Result};

/// ssh 的 UserKnownHostsFile 空设备: Windows 的 OpenSSH 不识别 /dev/null, 要用 NUL
pub const SSH_KNOWN_HOSTS_OPT: &str = if cfg!(windows) {
    "UserKnownHostsFile=NUL"
} else {
    "UserKnownHostsFile=/dev/null"
};

/// 每条 ssh 都带的保活: 15 秒一次 keepalive, 连续 4 次没回就断 (对端死了一分钟内知道),
/// 连接本身 20 秒建不起来就放弃。只能发现「连接死了」—— 2026-09-28 那次对端 sshd 活着、
/// 只是永远不关 channel, 保活照样有回音, 所以另有 `run_with_deadline` 的总时限兜底。
pub const SSH_KEEPALIVE_OPTS: [&str; 3] = [
    "ServerAliveInterval=15",
    "ServerAliveCountMax=4",
    "ConnectTimeout=20",
];

/// 远程命令默认的总时限。给整条 deploy 里最长的那种活 (节点上 docker build) 留足余量;
/// `OPS_SSH_TIMEOUT_SECS` 可改。不是「一般要多久」, 是「过了这个点一定是挂了」。
pub fn default_ssh_deadline() -> std::time::Duration {
    let secs = std::env::var("OPS_SSH_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&s| s > 0)
        .unwrap_or(60 * 60);
    std::time::Duration::from_secs(secs)
}

/// 跑一个子进程, 到点还没结束就杀掉并报错 —— 一个永远不返回的远程命令不能把调用方
/// (以及跑它的 CI) 一起挂住。`capture` 时收集 stdout/stderr (后台线程读, 管道写满也不会卡死),
/// 否则继承当前终端。`stdin` 有值时写进去再关掉。退出码不在这里判, 交给调用方。
pub fn run_with_deadline(
    cmd: &mut std::process::Command,
    deadline: std::time::Duration,
    capture: bool,
    stdin: Option<&[u8]>,
) -> Result<std::process::Output> {
    use std::io::{Read, Write};
    use std::process::Stdio;

    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    if capture {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    } else {
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    }
    let mut child = cmd.spawn()?;
    if let (Some(data), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(data)?;
    }
    let reader = |p: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = p {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out = reader(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = reader(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));

    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!("timed out after {}s and was killed", deadline.as_secs()));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    Ok(std::process::Output {
        status,
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}

/// 收紧私钥临时文件权限。Unix 下 chmod 600 (OpenSSH 强制要求);
/// Windows 下 %TEMP% 的 ACL 默认仅限当前用户, Win32-OpenSSH 直接接受, 无需处理。
pub fn secure_key_permissions(file: &std::fs::File) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = file.metadata()?.permissions();
        perms.set_mode(0o600);
        file.set_permissions(perms)?;
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

/// Target type that supports both Node IDs and App targets
#[derive(Debug, Clone)]
pub enum Target {
    /// Direct node ID (e.g., "12345" or "12345:/path")
    NodeId {
        id: u64,
        path: Option<String>,
    },
    /// App target (e.g., "api.RedQ" or "api.RedQ:/path")
    AppTarget {
        app: String,
        project: String,
        path: Option<String>,
    },
}

impl Target {
    /// Get the path if any
    pub fn path(&self) -> Option<&str> {
        match self {
            Target::NodeId { path, .. } => path.as_deref(),
            Target::AppTarget { path, .. } => path.as_deref(),
        }
    }

    /// Get the domain for this target
    pub fn domain(&self) -> String {
        match self {
            Target::NodeId { id, .. } => format!("{}.node.ops.autos", id),
            Target::AppTarget { app, project, .. } => format!("{}.{}.ops.autos", app, project),
        }
    }

    /// Check if this is a node ID target
    pub fn is_node_id(&self) -> bool {
        matches!(self, Target::NodeId { .. })
    }
}

/// Parse a target string into Target
/// Supports:
/// - "12345" → NodeId
/// - "12345:/path" → NodeId with path
/// - "api.RedQ" → AppTarget
/// - "api.RedQ:/path" → AppTarget with path
pub fn parse_target(target_str: &str) -> Result<Target> {
    // 1. Split off the path (after colon)
    let (server_part, path_part) = match target_str.split_once(':') {
        Some((s, p)) => (s, Some(p.to_string())),
        None => (target_str, None),
    };

    // 2. Check if it's a pure node ID (numeric)
    if let Ok(id) = server_part.parse::<u64>() {
        return Ok(Target::NodeId { id, path: path_part });
    }

    // 3. Parse as app.project format
    let parts: Vec<&str> = server_part.split('.').collect();
    if parts.len() != 2 {
        return Err(anyhow!(
            "Invalid target format. Expected 'app.project' (e.g., api.RedQ) or node ID (e.g., 12345)"
        ));
    }

    Ok(Target::AppTarget {
        app: parts[0].to_string(),
        project: parts[1].to_string(),
        path: path_part,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-09-28: 一条 `systemctl reload caddy` 在远端挂了 30 多分钟, CI 一起挂着。
    /// 到点的命令必须被杀掉并报错, 不能等它自己回来。
    #[cfg(unix)]
    #[test]
    fn a_command_past_its_deadline_is_killed_not_waited_for() {
        let started = std::time::Instant::now();
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("5");
        let r = run_with_deadline(&mut cmd, std::time::Duration::from_secs(1), true, None);
        assert!(r.is_err(), "a hung command is an error, not an eventual success");
        assert!(started.elapsed() < std::time::Duration::from_secs(3), "killed at the deadline, took {:?}", started.elapsed());
    }

    #[cfg(unix)]
    #[test]
    fn output_and_stdin_still_flow_within_the_deadline() {
        let mut cmd = std::process::Command::new("cat");
        let out = run_with_deadline(&mut cmd, std::time::Duration::from_secs(5), true, Some(b"hello")).unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, b"hello");
    }

    #[test]
    fn keepalive_notices_a_dead_peer_within_a_minute() {
        assert!(SSH_KEEPALIVE_OPTS.contains(&"ServerAliveInterval=15"));
        assert!(SSH_KEEPALIVE_OPTS.contains(&"ServerAliveCountMax=4"));
    }

    #[test]
    fn test_parse_target_node_id() {
        let result = parse_target("12345").unwrap();
        match result {
            Target::NodeId { id, path } => {
                assert_eq!(id, 12345);
                assert!(path.is_none());
            }
            _ => panic!("Expected NodeId"),
        }
    }

    #[test]
    fn test_parse_target_node_id_with_path() {
        let result = parse_target("12345:/root/").unwrap();
        match result {
            Target::NodeId { id, path } => {
                assert_eq!(id, 12345);
                assert_eq!(path.as_deref(), Some("/root/"));
            }
            _ => panic!("Expected NodeId"),
        }
    }

    #[test]
    fn test_parse_target_app_target() {
        let result = parse_target("api.RedQ").unwrap();
        match result {
            Target::AppTarget { app, project, path } => {
                assert_eq!(app, "api");
                assert_eq!(project, "RedQ");
                assert!(path.is_none());
            }
            _ => panic!("Expected AppTarget"),
        }
    }

    #[test]
    fn test_parse_target_app_target_with_path() {
        let result = parse_target("api.RedQ:/var/www").unwrap();
        match result {
            Target::AppTarget { app, project, path } => {
                assert_eq!(app, "api");
                assert_eq!(project, "RedQ");
                assert_eq!(path.as_deref(), Some("/var/www"));
            }
            _ => panic!("Expected AppTarget"),
        }
    }

    #[test]
    fn test_target_domain() {
        let node = Target::NodeId { id: 12345, path: None };
        assert_eq!(node.domain(), "12345.node.ops.autos");

        let app = Target::AppTarget {
            app: "api".to_string(),
            project: "RedQ".to_string(),
            path: None,
        };
        assert_eq!(app.domain(), "api.RedQ.ops.autos");
    }
}
