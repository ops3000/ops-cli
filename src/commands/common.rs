use crate::{api, config, utils};
use anyhow::{Context, Result};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

/// 解析 "$ENV_VAR" → 读环境变量值
pub fn resolve_env_value(val: &str) -> Result<String> {
    if val.starts_with('$') {
        std::env::var(&val[1..])
            .with_context(|| format!("Environment variable {} not set", val))
    } else {
        Ok(val.to_string())
    }
}

/// rsync 同步本地代码到远程服务器
pub async fn rsync_push(target_str: &str, deploy_path: &str) -> Result<()> {
    let target = utils::parse_target(target_str)?;
    let full_domain = format!("{}.{}.ops.autos", target.environment, target.project);

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    let key_resp = api::get_ci_private_key(&token, &target.project, &target.environment).await?;

    let mut temp_key_file = tempfile::NamedTempFile::new()?;
    writeln!(temp_key_file, "{}", key_resp.private_key)?;
    let meta = temp_key_file.as_file().metadata()?;
    let mut perms = meta.permissions();
    perms.set_mode(0o600);
    temp_key_file.as_file().set_permissions(perms)?;
    let key_path = temp_key_file.path().to_str().unwrap().to_string();

    let ssh_cmd = format!(
        "ssh -i {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR",
        key_path
    );
    let remote = format!("root@{}:{}/", full_domain, deploy_path);

    o_debug!("   ./ → {}", remote);

    let status = Command::new("rsync")
        .arg("-az")
        .arg("--delete")
        .arg("-e").arg(&ssh_cmd)
        .arg("--exclude").arg("target/")
        .arg("--exclude").arg("node_modules/")
        .arg("--exclude").arg(".git/")
        .arg("--exclude").arg(".env")
        .arg("--exclude").arg(".env.deploy")
        .arg("./")
        .arg(&remote)
        .status()
        .context("Failed to execute rsync (is rsync installed?)")?;

    if !status.success() {
        return Err(anyhow::anyhow!("rsync failed with status: {}", status));
    }
    Ok(())
}
