use crate::commands::deploy::load_ops_toml;
use crate::commands::ssh;
use crate::{api, config};
use anyhow::{Context, Result};

pub async fn handle_logs(file: String, service: String, tail: u32, follow: bool) -> Result<()> {
    let config = load_ops_toml(&file)?;

    let project = &config.project;
    let app = config.apps.first()
        .map(|a| a.name.as_str())
        .unwrap_or(project.as_str());

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    let resp = api::get_app_deploy_targets(&token, project, app).await
        .context("Failed to get deploy targets")?;
    let t = resp.targets.first()
        .context("No nodes bound")?;

    let follow_flag = if follow { " -f" } else { "" };
    let cmd = format!(
        "cd {} && docker compose logs --tail={}{} {}",
        config.deploy_path, tail, follow_flag, service
    );

    ssh::handle_ssh(t.domain.clone(), Some(cmd)).await?;
    Ok(())
}
