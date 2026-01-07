use crate::{api, config, utils};
use anyhow::{Context, Result};
use std::process::{Command, Stdio};
use colored::Colorize;
use std::io::Write; 
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;

pub async fn handle_ssh(target_str: String, command: Option<String>) -> Result<()> {
    let target = utils::parse_target(&target_str)?;
    let full_domain = format!("{}.{}.ops.autos", target.environment, target.project);
    let ssh_target = format!("root@{}", full_domain);
    
    // 1. 获取凭证
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    println!("Fetching access credentials...");
    let ci_key_res = api::get_ci_private_key(&token, &target.project, &target.environment).await;

    // 2. 准备私钥文件
    let mut temp_key_file = tempfile::NamedTempFile::new()?;
    
    match ci_key_res {
        Ok(key_resp) => {
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
               .arg("-o").arg("UserKnownHostsFile=/dev/null");

            if let Some(remote_cmd) = command {
                // === 执行远程命令模式 ===
                println!("Executing on {}...", ssh_target.cyan());
                cmd.arg(&ssh_target).arg(&remote_cmd);
                
                let output = cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit()).output()?;
                if !output.status.success() {
                    return Err(anyhow::anyhow!("Remote command failed with status: {}", output.status));
                }
            } else {
                // === 交互式 Shell 模式 ===
                println!("Connecting...");
                cmd.arg(&ssh_target);
                
                let mut child = cmd.spawn().context("Failed to launch ssh command")?;
                child.wait()?;
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