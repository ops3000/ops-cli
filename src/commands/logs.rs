use crate::commands::deploy::load_ops_toml;
use crate::commands::ssh;
use anyhow::Result;

pub async fn handle_logs(file: String, service: String, tail: u32, follow: bool) -> Result<()> {
    let config = load_ops_toml(&file)?;

    let follow_flag = if follow { " -f" } else { "" };
    let cmd = format!(
        "cd {} && docker compose logs --tail={}{} {}",
        config.deploy_path, tail, follow_flag, service
    );

    // 用 handle_ssh 支持 -f 的实时流式输出
    ssh::handle_ssh(config.target.clone(), Some(cmd)).await?;
    Ok(())
}
