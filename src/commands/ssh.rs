use crate::{api, config, utils};
use anyhow::{Context, Result};
use std::process::Command;
use colored::Colorize;
use std::io::Write; // 必须引入 Write trait 才能使用 writeln!
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt; // 注意：这在 Windows 上可能无法编译，建议在 Linux/macOS 下使用

pub async fn handle_ssh(target_str: String) -> Result<()> {
    let target = utils::parse_target(&target_str)?;
    let full_domain = format!("{}.{}.ops.autos", target.environment, target.project);
    let ssh_target = format!("root@{}", full_domain);
    
    println!("Preparing to connect to {}...", ssh_target.cyan());

    // 1. 尝试加载配置和 Token
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    println!("Fetching access credentials...");
    
    // 2. 从 API 获取 CI 私钥
    let ci_key_res = api::get_ci_private_key(&token, &target.project, &target.environment).await;

    // 创建临时文件来存放私钥
    let mut temp_key_file = tempfile::NamedTempFile::new()?;
    
    match ci_key_res {
        Ok(key_resp) => {
            // --- 修改重点：使用 writeln! 确保末尾有换行符 ---
            // SSH 客户端对私钥文件的格式非常敏感，如果末尾没有换行符可能会报 "invalid format"
            writeln!(temp_key_file, "{}", key_resp.private_key)?;
            
            // 设置权限为 600 (rw-------)，否则 ssh 会拒绝使用
            let meta = temp_key_file.as_file().metadata()?;
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            temp_key_file.as_file().set_permissions(perms)?;

            println!("{}", "✔ Access granted via CI Key.".green());
            
            // 3. 使用指定的私钥文件启动 SSH
            let key_path = temp_key_file.path().to_str().unwrap();

            println!("Connecting...");
            let mut child = Command::new("ssh")
                .arg("-i")
                .arg(key_path) // 指定私钥
                .arg("-o").arg("StrictHostKeyChecking=no") 
                .arg("-o").arg("UserKnownHostsFile=/dev/null") 
                .arg(&ssh_target)
                .spawn()
                .context("Failed to launch ssh command")?;

            let status = child.wait()?;
            if !status.success() {
                // ssh exited with error
            }
        },
        Err(e) => {
            println!("{}", format!("⚠ Could not fetch CI key: {}. Falling back to system SSH.", e).yellow());
            
            let mut child = Command::new("ssh")
                .arg(&ssh_target)
                .spawn()
                .context("Failed to launch ssh command")?;
            
            child.wait()?;
        }
    }
    
    Ok(())
}