use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use crate::{api, config};

/// Parse target in "app.project" format
fn parse_target(target: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = target.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Target must be in 'app.project' format (e.g., api.RedQ)"));
    }
    Ok((parts[1].to_string(), parts[0].to_string()))
}

pub async fn handle_status(target: String) -> Result<()> {
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    let (project, app) = parse_target(&target)?;

    println!("{} Pool status for {}\n", "🏊".cyan(), target.green());

    let resp = api::get_app_deploy_targets(&token, &project, &app).await?;

    println!("  Mode:     {}", resp.mode.cyan());
    if let Some(ref strategy) = resp.lb_strategy {
        println!("  Strategy: {}", strategy.cyan());
    }
    if let Some(gid) = resp.node_group_id {
        println!("  Group ID: {}", gid.to_string().cyan());
    }
    println!();

    if resp.targets.is_empty() {
        println!("  No nodes bound to this app.");
        return Ok(());
    }

    // Table header
    println!("  {:<8} {:<28} {:<16} {:<14} {:<10} {:<8}",
        "ID", "Domain", "IP", "Region", "Status", "Primary");
    println!("  {}", "-".repeat(84));

    for t in &resp.targets {
        let status_colored = match t.status.as_str() {
            "healthy" => t.status.green(),
            "draining" => t.status.yellow(),
            _ => t.status.red(),
        };
        let primary = if t.is_primary { "yes".green() } else { "-".normal() };
        let region = t.region.as_deref().unwrap_or("-");

        println!("  {:<8} {:<28} {:<16} {:<14} {:<10} {:<8}",
            t.node_id, t.domain, t.ip_address, region, status_colored, primary);
    }

    let healthy = resp.targets.iter().filter(|t| t.status == "healthy").count();
    let total = resp.targets.len();
    println!("\n  {}/{} nodes healthy", healthy.to_string().green(), total);

    Ok(())
}

pub async fn handle_strategy(target: String, strategy: String) -> Result<()> {
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    let valid = ["round-robin", "geo", "weighted", "failover"];
    if !valid.contains(&strategy.as_str()) {
        return Err(anyhow!("Invalid strategy '{}'. Must be one of: {}", strategy, valid.join(", ")));
    }

    let (project, app) = parse_target(&target)?;

    // Get deploy targets to find the node group ID
    let resp = api::get_app_deploy_targets(&token, &project, &app).await?;
    let group_id = resp.node_group_id
        .context("App is in single-node mode. Bind a second node to enable pool mode.")?;

    println!("{} Updating strategy for {} to {}...", "🔄".cyan(), target.green(), strategy.yellow());

    api::update_node_group_strategy(&token, group_id, &strategy).await?;

    println!("{} Strategy updated to {}", "✔".green(), strategy.green());
    Ok(())
}

pub async fn handle_drain(target: String, node_id: u64) -> Result<()> {
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    let (project, app) = parse_target(&target)?;

    // Get deploy targets to find the node group ID
    let resp = api::get_app_deploy_targets(&token, &project, &app).await?;
    let group_id = resp.node_group_id
        .context("App is in single-node mode. Cannot drain.")?;

    println!("{} Draining node {} from {}...", "🚰".cyan(), node_id.to_string().yellow(), target.green());

    api::drain_node(&token, group_id, node_id).await?;

    println!("{} Node {} is now draining (no new traffic will be routed)", "✔".green(), node_id);
    Ok(())
}

pub async fn handle_undrain(target: String, node_id: u64) -> Result<()> {
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    let (project, app) = parse_target(&target)?;

    // Get deploy targets to find the node group ID
    let resp = api::get_app_deploy_targets(&token, &project, &app).await?;
    let group_id = resp.node_group_id
        .context("App is in single-node mode. Cannot undrain.")?;

    println!("{} Restoring node {} in {}...", "🔄".cyan(), node_id.to_string().yellow(), target.green());

    api::undrain_node(&token, group_id, node_id).await?;

    println!("{} Node {} is back in rotation", "✔".green(), node_id);
    Ok(())
}
