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
    let authorized_keys_path = ssh_dir.join("authorized_keys");

    // 确保目录存在
    fs::create_dir_all(&ssh_dir)?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&authorized_keys_path)
        .with_context(|| format!("Failed to open authorized_keys file at {:?}", authorized_keys_path))?;

    writeln!(file, "\n# Added by ops.autos CLI for CI/CD")?;
    writeln!(file, "{}", pubkey)?;

    Ok(())
}