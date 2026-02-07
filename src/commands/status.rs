use crate::commands::deploy::load_ops_toml;
use crate::commands::ssh;
use crate::{api, config};
use anyhow::{Context, Result};
use colored::Colorize;

pub async fn handle_status(file: String) -> Result<()> {
    let ops_config = load_ops_toml(&file)?;

    let project = ops_config.project.as_deref()
        .or(ops_config.app.as_deref());
    let app = ops_config.app.as_deref()
        .or(ops_config.project.as_deref());

    // Try multi-node status if we have project+app and a token
    if let (Some(project), Some(app)) = (project, app) {
        if let Ok(cfg) = config::load_config() {
            if let Some(ref token) = cfg.token {
                if let Ok(resp) = api::get_app_deploy_targets(token, project, app).await {
                    if resp.targets.len() > 1 {
                        return show_multi_node_status(&ops_config, &resp).await;
                    }
                }
            }
        }
    }

    // Fallback: single-node status via SSH
    let target = ops_config.target.as_deref()
        .context("ops.toml must have 'target' for status command")?;

    println!("{} {}\n", "📊 Status:".cyan(), target.green());

    let cmd = format!(
        "cd {} && docker compose ps",
        ops_config.deploy_path
    );

    ssh::execute_remote_command(target, &cmd, None).await?;
    Ok(())
}

async fn show_multi_node_status(
    config: &crate::types::OpsToml,
    resp: &crate::types::DeployTargetsResponse,
) -> Result<()> {
    let app = config.app.as_deref().or(config.project.as_deref()).unwrap_or("?");
    let project = config.project.as_deref().or(config.app.as_deref()).unwrap_or("?");

    let strategy = resp.lb_strategy.as_deref().unwrap_or("single");
    let node_count = resp.targets.len();

    println!("{} {} (project: {})", "📊 App:".cyan(), app.green(), project.green());
    println!("   Mode: {} ({} nodes, strategy: {})\n",
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

        println!("  Node {} ({}, {}){}", t.node_id, region, hostname, primary_tag);

        // Try to get container status via SSH
        let cmd = format!("cd {} && docker compose ps --format '  {{{{.Name}}}}\\t{{{{.Status}}}}'",
            config.deploy_path);
        print!("    Status: ");
        match ssh::execute_remote_command(&t.domain, &cmd, None).await {
            Ok(_) => {}
            Err(_) => {
                // If SSH fails, just show the health status
                println!("    {}", status_colored);
            }
        }
        println!();
    }

    let healthy = resp.targets.iter().filter(|t| t.status == "healthy").count();
    println!("  {}/{} nodes healthy", healthy.to_string().green(), node_count);

    Ok(())
}
