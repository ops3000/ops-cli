use crate::{api, config, utils};
use crate::utils::TargetType;
use anyhow::{Context, Result};
use std::process::{Command, Stdio};
use colored::Colorize;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;

/// 这是一个通用的 SSH 命令构建器，其他模块可以复用
/// Supports both Node ID (e.g., "12345") and App target (e.g., "api.RedQ")
pub async fn build_ssh_command(target_str: &str) -> Result<(Command, tempfile::NamedTempFile)> {
    let target = utils::parse_target_v2(target_str)?;
    let full_domain = target.domain();
    let ssh_target = format!("root@{}", full_domain);

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    o_debug!("Fetching access credentials...");

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
       .arg("-o").arg("LogLevel=ERROR")
       .arg(&ssh_target);

    Ok((cmd, temp_key_file))
}

/// 可复用的 SSH 会话，一次 fetch CI key，多次执行命令
pub struct SshSession {
    ssh_target: String,
    _temp_key_file: tempfile::NamedTempFile,
    key_path: String,
    target_str: String,
}

impl SshSession {
    /// 建立会话：fetch CI key，创建 temp key file（只做一次）
    pub async fn connect(target_str: &str) -> Result<Self> {
        let target = utils::parse_target_v2(target_str)?;
        let full_domain = target.domain();
        let ssh_target = format!("root@{}", full_domain);

        let cfg = config::load_config().context("Config error")?;
        let token = cfg.token.context("Please run `ops login` first.")?;

        o_debug!("Fetching access credentials...");

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

        let mut temp_key_file = tempfile::NamedTempFile::new()?;
        writeln!(temp_key_file, "{}", private_key)?;
        let meta = temp_key_file.as_file().metadata()?;
        let mut perms = meta.permissions();
        perms.set_mode(0o600);
        temp_key_file.as_file().set_permissions(perms)?;

        let key_path = temp_key_file.path().to_str().unwrap().to_string();

        o_debug!("{}", "✔ Access granted via CI Key.".green());

        Ok(Self { ssh_target, _temp_key_file: temp_key_file, key_path, target_str: target_str.to_string() })
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
           .arg("-o").arg("LogLevel=ERROR")
           .arg(&self.ssh_target);
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
    pub fn rsync_push(&self, remote_path: &str) -> Result<()> {
        let ssh_cmd = format!(
            "ssh -i {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR",
            self.key_path
        );
        let remote = format!("{}:{}/", self.ssh_target, remote_path);

        let status = Command::new("rsync")
            .arg("-az")
            .arg("--delete")
            .arg("-e").arg(&ssh_cmd)
            .arg("--exclude").arg("target/")
            .arg("--exclude").arg("node_modules/")
            .arg("--exclude").arg(".git/")
            .arg("--exclude").arg(".env")
            .arg("./")
            .arg(&remote)
            .status()
            .context("Failed to execute rsync (is rsync installed?)")?;

        if !status.success() {
            return Err(anyhow::anyhow!("rsync failed with status: {}", status));
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