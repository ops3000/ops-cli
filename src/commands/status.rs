use crate::commands::deploy::load_ops_toml;
use crate::commands::ssh;
use crate::{api, config};
use anyhow::{Context, Result};
use colored::Colorize;

pub async fn handle_status(file: String) -> Result<()> {
    let ops_config = load_ops_toml(&file)?;

    let project = &ops_config.project;
    let app = ops_config.apps.first()
        .map(|a| a.name.as_str())
        .unwrap_or(project.as_str());

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    let resp = api::get_app_deploy_targets(&token, project, app).await
        .context("Failed to get deploy targets")?;

    if resp.targets.is_empty() {
        anyhow::bail!("No nodes bound to app '{}' in project '{}'", app, project);
    }

    if resp.targets.len() > 1 {
        return show_multi_node_status(&ops_config, &resp).await;
    }

    // Single-node status via SSH
    let t = &resp.targets[0];
    o_step!("{} {}\n", "📊 Status:".cyan(), t.domain.green());

    let cmd = format!(
        "cd {} && docker compose ps",
        ops_config.deploy_path
    );

    ssh::execute_remote_command(&t.domain, &cmd, None).await?;
    Ok(())
}

async fn show_multi_node_status(
    config: &crate::types::OpsToml,
    resp: &crate::types::DeployTargetsResponse,
) -> Result<()> {
    let app = config.apps.first()
        .map(|a| a.name.as_str())
        .unwrap_or(config.project.as_str());
    let project = &config.project;

    let strategy = resp.lb_strategy.as_deref().unwrap_or("single");
    let node_count = resp.targets.len();

    o_step!("{} {} (project: {})", "📊 App:".cyan(), app.green(), project.green());
    o_detail!("   Mode: {} ({} nodes, strategy: {})\n",
        resp.mode.cyan(), node_count, strategy.yellow());

    for t in &resp.targets {
        let region = t.region.as_deref().unwrap_or("-");
        let hostname = t.hostname.as_deref().unwrap_or("");
        let status_colored = match t.status.as_str() {
            "healthy" => t.status.green(),
            "draining" => t.status.yellow(),
            _ => t.status.red(),
        };
        let primary_tag = if t.is_primary { " (primary)".cyan() } else { "".normal() };

        o_detail!("  Node {} ({}, {}){}", t.node_id, region, hostname, primary_tag);

        // Try to get container status via SSH
        let cmd = format!("cd {} && docker compose ps --format '  {{{{.Name}}}}\\t{{{{.Status}}}}'",
            config.deploy_path);
        o_print!("    Status: ");
        match ssh::execute_remote_command(&t.domain, &cmd, None).await {
            Ok(_) => {}
            Err(_) => {
                // If SSH fails, just show the health status
                o_detail!("    {}", status_colored);
            }
        }
        o_detail!("");
    }

    let healthy = resp.targets.iter().filter(|t| t.status == "healthy").count();
    o_result!("  {}/{} nodes healthy", healthy.to_string().green(), node_count);

    Ok(())
}
