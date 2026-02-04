use crate::{api, config};
use anyhow::{Context, Result};
use colored::Colorize;

/// List all nodes owned by the current user
pub async fn handle_list() -> Result<()> {
    let cfg = config::load_config()
        .context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token
        .context("You are not logged in. Please run `ops login` first.")?;

    let res = api::list_nodes_v2(&token).await?;

    if res.nodes.is_empty() {
        println!("{}", "No nodes found.".yellow());
        println!();
        println!("Initialize a node with: ops init --region <region>");
        return Ok(());
    }

    println!("{}", "My Nodes:".bold());
    println!();

    for node in res.nodes {
        let status_icon = match node.status.as_str() {
            "healthy" => "●".green(),
            "unhealthy" => "●".red(),
            "draining" => "◐".yellow(),
            "offline" => "○".red(),
            _ => "○".dimmed(),
        };

        let serve_status = if node.has_serve_token > 0 {
            "serve ✓".green()
        } else {
            "serve ✗".red()
        };

        let region_str = node.region.as_deref().unwrap_or("-");
        let hostname_str = node.hostname.as_deref().unwrap_or(&node.ip_address);

        println!(
            "  {} {} {} ({}) region:{} [{}]",
            status_icon,
            format!("#{}", node.id).dimmed(),
            hostname_str.cyan().bold(),
            node.ip_address,
            region_str,
            serve_status
        );
        println!("      Domain: {}", node.domain.dimmed());

        if let Some(last_check) = node.last_health_check {
            println!("      Last check: {}", last_check.dimmed());
        }
    }

    println!();
    println!("{}", "Use 'ops node info <id>' for details".dimmed());

    Ok(())
}

/// Show detailed information about a specific node
pub async fn handle_info(node_id: u64) -> Result<()> {
    let cfg = config::load_config()
        .context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token
        .context("You are not logged in. Please run `ops login` first.")?;

    let node = api::get_node_v2(&token, node_id).await?;

    let status_icon = match node.status.as_str() {
        "healthy" => "●".green(),
        "unhealthy" => "●".red(),
        "draining" => "◐".yellow(),
        "offline" => "○".red(),
        _ => "○".dimmed(),
    };

    println!("{}", format!("Node #{}", node.id).bold());
    println!();
    println!("  Status:      {} {}", status_icon, node.status);
    println!("  Domain:      {}", node.domain.cyan());
    println!("  IP Address:  {}", node.ip_address);

    if let Some(hostname) = node.hostname {
        println!("  Hostname:    {}", hostname);
    }
    if let Some(region) = node.region {
        println!("  Region:      {}", region);
    }
    if let Some(zone) = node.zone {
        println!("  Zone:        {}", zone);
    }

    println!("  Serve Port:  {}", node.serve_port);
    println!("  Created:     {}", node.created_at);

    if let Some(last_check) = node.last_health_check {
        println!("  Last Check:  {}", last_check);
    }

    // Allowed projects/apps
    if let Some(projects) = node.allowed_projects {
        println!("  Allowed Projects: {}", projects.join(", ").cyan());
    } else {
        println!("  Allowed Projects: {}", "all".dimmed());
    }

    if let Some(apps) = node.allowed_apps {
        println!("  Allowed Apps: {}", apps.join(", ").cyan());
    } else {
        println!("  Allowed Apps: {}", "all".dimmed());
    }

    // Bound apps
    if let Some(bound_apps) = node.bound_apps {
        if !bound_apps.is_empty() {
            println!();
            println!("{}", "Bound Apps:".bold());
            for app in bound_apps {
                let primary = if app.is_primary.unwrap_or(0) > 0 {
                    " (primary)".green()
                } else {
                    "".normal()
                };
                println!("  • {}.{}{}", app.name.cyan(), app.project_name, primary);
            }
        }
    }

    println!();
    println!("{}", "Commands:".yellow());
    println!("  SSH:    ops ssh {}", node_id);
    println!("  Ping:   ops ping {}", node_id);
    println!("  Delete: ops node remove {}", node_id);

    Ok(())
}

/// Remove a node
pub async fn handle_remove(node_id: u64, force: bool) -> Result<()> {
    let cfg = config::load_config()
        .context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token
        .context("You are not logged in. Please run `ops login` first.")?;

    if !force {
        println!("{}", format!("This will delete node #{} and all its associated data.", node_id).yellow());
        println!("The node's DNS record will also be removed.");
        println!();
        println!("Use --force to confirm deletion.");
        return Ok(());
    }

    println!("Deleting node #{}...", node_id);

    let res = api::delete_node_v2(&token, node_id).await?;

    println!("{}", format!("✔ {}", res.message).green());

    Ok(())
}
