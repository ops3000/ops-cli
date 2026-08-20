// src/ssh.rs

use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::io::Write;
use std::process::Command;
use colored::Colorize;

fn get_ssh_dir() -> Result<PathBuf> {
    dirs::home_dir()
        .context("Could not find home directory")
        .map(|p| p.join(".ssh"))
}

/// sshd 到底让不让 root 用 key 登录。
///
/// `sshd -T` 打印的是所有 Include 都展开之后**最终生效**的配置, 所以它是这个
/// 问题唯一权威的答案 —— 云镜像普遍把设置放在 `/etc/ssh/sshd_config.d/*.conf`
/// 里, 只读主配置文件会看漏。需要 root 才能跑, 而 `ops init` 本来就是 root。
///
/// 拿不准时返回 true (保持既有行为): 这个函数只用来决定"要不要额外把 key 也
/// 装给普通用户", 猜错的代价是多装一份 key, 而猜错另一边的代价是节点根本连不上。
#[cfg(unix)]
fn root_login_permitted() -> bool {
    let out = match Command::new("sshd").arg("-T").output() {
        Ok(o) if o.status.success() => o,
        // sshd 不在 PATH (常见于 /usr/sbin 不在非登录 shell 的 PATH 里) —— 再试一次绝对路径
        _ => match Command::new("/usr/sbin/sshd").arg("-T").output() {
            Ok(o) if o.status.success() => o,
            _ => return true,
        },
    };
    let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("permitrootlogin ") {
            // yes / prohibit-password / without-password / forced-commands-only
            // 里只有 "no" 是彻底关门。forced-commands-only 也不能跑任意命令,
            // 对 `ops ssh` 而言等同于不可用。
            let v = v.trim();
            return v != "no" && v != "forced-commands-only";
        }
    }
    true
}

#[cfg(not(unix))]
fn root_login_permitted() -> bool {
    true
}

/// 谁将来会用 `ops ssh` 登录这台机器。
///
/// `None` = root, 这是自建服务器上的常态, 也是一直以来的假设。但那个假设在
/// 云镜像上是错的: GCP / AWS / 多数发行版云镜像默认 `PermitRootLogin no`,
/// 于是 `ops init` 把 CI key 老老实实装进 `/root/.ssh/authorized_keys` ——
/// 位置完全正确, 权限完全正确, 而那个用户永远不被允许登录。表现出来就是
/// `ops ssh` 报 `Permission denied (publickey)`, 一个看起来像"key 没装对"
/// 的错误, 于是人们去查 key, 而 key 一直是好的。
///
/// (2026-08-20 一台 GCP Debian 13 机器上实测:
///  `sshd -T` → `permitrootlogin no`, `/root/.ssh/authorized_keys` 里
///  `# Added by ops.autos CLI for CI/CD` 那把 key 一字不差地躺着。)
///
/// root 不能登录时, 目标就是**实际执行 init 的那个人** —— 他刚刚才用自己的
/// 账号 ssh 进来跑了这条命令, 所以他的登录方式是确定可用的。
#[cfg(unix)]
pub fn ci_key_login_user() -> Option<String> {
    if root_login_permitted() {
        return None;
    }
    // sudo 跑的 → SUDO_USER 是真人; 直接以 root 跑的 → 没有别人可选。
    std::env::var("SUDO_USER").ok().filter(|u| !u.is_empty() && u != "root")
}

#[cfg(not(unix))]
pub fn ci_key_login_user() -> Option<String> {
    None
}

/// `user` 的家目录, 问 passwd 而不是拼 `/home/<name>`。
#[cfg(unix)]
fn home_of(user: &str) -> Option<PathBuf> {
    let out = Command::new("getent").arg("passwd").arg(user).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout);
    // name:x:uid:gid:gecos:home:shell
    line.trim().split(':').nth(5).filter(|s| !s.is_empty()).map(PathBuf::from)
}

/// Windows 上到不了这里 —— `ci_key_login_user` 恒为 None, 调用点进不去这个
/// 分支。存在只是为了让它编译得过。
#[cfg(not(unix))]
fn home_of(_user: &str) -> Option<PathBuf> {
    None
}

pub fn ensure_ssh_key_exists() -> Result<PathBuf> {
    let ssh_dir = get_ssh_dir()?;
    let priv_key_path = ssh_dir.join("id_rsa");
    let pub_key_path = ssh_dir.join("id_rsa.pub");

    if !pub_key_path.exists() {
        println!("{}", "No SSH key found. Generating a new one for you...".yellow());
        
        // 确保 .ssh 目录存在
        fs::create_dir_all(&ssh_dir)?;

        // 调用 ssh-keygen
        // -t rsa: 类型
        // -b 4096: 长度
        // -f path: 文件路径
        // -N "": 空密码 (实现免密/自动化关键)
        let output = Command::new("ssh-keygen")
            .arg("-t").arg("rsa")
            .arg("-b").arg("4096")
            .arg("-f").arg(priv_key_path.to_str().unwrap())
            .arg("-N").arg("")
            .output()
            .context("Failed to execute ssh-keygen")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("ssh-keygen failed: {}", stderr));
        }

        println!("{}", "✔ New SSH key generated.".green());
    }

    Ok(pub_key_path)
}

pub fn get_default_pubkey() -> Result<String> {
    let pubkey_path = ensure_ssh_key_exists()?;
        
    let content = fs::read_to_string(&pubkey_path)
        .with_context(|| format!("Failed to read SSH public key from {:?}", pubkey_path))?;
        
    Ok(content.trim().to_string())
}

pub fn add_to_authorized_keys(pubkey: &str) -> Result<()> {
    let ssh_dir = get_ssh_dir()?;
    write_authorized_key(&ssh_dir, pubkey)?;

    // root 登录被禁的机器 (云镜像的常态) 上, 装进 /root 的 key 是一把装在
    // 永不开门的锁上的钥匙。再给真正能登录的那个人装一份, 并且**说出来** ——
    // 从前这里是静默的, 于是失败发生在几分钟后的另一台机器上, 表现为
    // `Permission denied (publickey)`, 而 key 一直好好的。
    if let Some(user) = ci_key_login_user() {
        println!();
        println!(
            "{}",
            "  ⚠ sshd on this machine has PermitRootLogin=no (the default on most cloud images)."
                .yellow()
        );
        match home_of(&user) {
            Some(home) => {
                let dir = home.join(".ssh");
                write_authorized_key(&dir, pubkey)?;
                chown_to(&dir, &user);
                println!("    Installed the CI key for '{}' as well, and registered", user.cyan());
                println!("    it as this node's SSH login user.");

                // GCP / AWS 的 guest agent 每分钟从实例 metadata 重写
                // ~/.ssh/authorized_keys, 手写进去的 key 会被抹掉 —— 静默地,
                // 而且是在几分钟之后, 所以它看起来像"时好时坏"。
                if guest_agent_present() {
                    println!();
                    println!(
                        "{}",
                        "  ⚠ A cloud guest agent is running. It rewrites ~/.ssh/authorized_keys"
                            .yellow()
                    );
                    println!(
                        "{}",
                        "    from instance metadata every minute, so the key just installed will"
                            .yellow()
                    );
                    println!("{}", "    likely be wiped. Make it stick one of two ways:".yellow());
                    println!("      1. add the key to the instance's SSH keys in the cloud console, or");
                    println!(
                        "      2. allow key-based root login (matching how ops drives its other nodes):"
                    );
                    println!(
                        "{}",
                        "         echo 'PermitRootLogin prohibit-password' | sudo tee \
                         /etc/ssh/sshd_config.d/60-ops-root.conf && sudo systemctl restart ssh"
                            .cyan()
                    );
                }
            }
            None => {
                println!(
                    "{}",
                    format!(
                        "    '{user}' has no home directory, so there is nowhere to put a \
                         working key.\n    `ops ssh` to this node will fail until you allow \
                         root login or install a key by hand."
                    )
                    .yellow()
                );
            }
        }
        println!();
    }

    Ok(())
}

/// 云厂商的 guest agent 在不在 —— 它会按 metadata 重写 authorized_keys。
/// 只用来决定要不要多说一句话, 所以宁可漏报不误报。
#[cfg(unix)]
fn guest_agent_present() -> bool {
    [
        "/usr/bin/google_guest_agent",
        "/usr/bin/google_authorized_keys",
        "/usr/bin/amazon-ssm-agent",
        "/usr/bin/cloud-init",
    ]
    .iter()
    .any(|p| std::path::Path::new(p).exists())
}

#[cfg(not(unix))]
fn guest_agent_present() -> bool {
    false
}

/// 往一个 `.ssh` 目录追加一把授权 key。目录不存在就建 (0700)。
fn write_authorized_key(ssh_dir: &PathBuf, pubkey: &str) -> Result<()> {
    fs::create_dir_all(ssh_dir)
        .with_context(|| format!("Failed to create {:?}", ssh_dir))?;
    let path = ssh_dir.join("authorized_keys");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("Failed to open authorized_keys file at {:?}", path))?;
    writeln!(file, "\n# Added by ops.autos CLI for CI/CD")?;
    writeln!(file, "{}", pubkey)?;
    Ok(())
}

/// 把 `.ssh` 及其内容划给 `user`。我们是以 root 在写别人的家目录, 属主不对
/// sshd 会因为 StrictModes 直接忽略这个文件 —— 那等于白装。
#[cfg(unix)]
fn chown_to(dir: &PathBuf, user: &str) {
    let _ = Command::new("chown")
        .arg("-R")
        .arg(format!("{user}:{user}"))
        .arg(dir)
        .status();
    let _ = Command::new("chmod").arg("700").arg(dir).status();
    let _ = Command::new("chmod").arg("600").arg(dir.join("authorized_keys")).status();
}

#[cfg(not(unix))]
fn chown_to(_dir: &PathBuf, _user: &str) {}