use crate::{api, config, prompt};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;

/// List all nodes owned by the current user
pub async fn handle_list() -> Result<()> {
    let cfg = config::load_config()
        .context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token
        .context("You are not logged in. Please run `ops login` first.")?;

    let res = api::list_nodes(&token).await?;

    if res.nodes.is_empty() {
        o_warn!("{}", "No nodes found.".yellow());
        o_detail!();
        o_detail!("Initialize a node with: ops init --region <region>");
        return Ok(());
    }

    o_step!("{}", "My Nodes:".bold());
    o_detail!();

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

        o_detail!(
            "  {} {} {} ({}) region:{} [{}]",
            status_icon,
            format!("#{}", node.id).dimmed(),
            hostname_str.cyan().bold(),
            node.ip_address,
            region_str,
            serve_status
        );
        o_detail!("      Domain: {}", node.domain.dimmed());

        if let Some(last_check) = node.last_health_check {
            o_detail!("      Last check: {}", last_check.dimmed());
        }
    }

    o_detail!();
    o_detail!("{}", "Use 'ops node info <id>' for details".dimmed());

    Ok(())
}

/// Show detailed information about a specific node
pub async fn handle_info(node_id: u64) -> Result<()> {
    let cfg = config::load_config()
        .context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token
        .context("You are not logged in. Please run `ops login` first.")?;

    let node = api::get_node(&token, node_id).await?;

    let status_icon = match node.status.as_str() {
        "healthy" => "●".green(),
        "unhealthy" => "●".red(),
        "draining" => "◐".yellow(),
        "offline" => "○".red(),
        _ => "○".dimmed(),
    };

    o_step!("{}", format!("Node #{}", node.id).bold());
    o_detail!();
    o_detail!("  Status:      {} {}", status_icon, node.status);
    o_detail!("  Domain:      {}", node.domain.cyan());
    o_detail!("  IP Address:  {}", node.ip_address);

    if let Some(hostname) = node.hostname {
        o_detail!("  Hostname:    {}", hostname);
    }
    if let Some(region) = node.region {
        o_detail!("  Region:      {}", region);
    }
    if let Some(zone) = node.zone {
        o_detail!("  Zone:        {}", zone);
    }

    o_detail!("  Serve Port:  {}", node.serve_port);
    o_detail!("  Created:     {}", node.created_at);

    if let Some(last_check) = node.last_health_check {
        o_detail!("  Last Check:  {}", last_check);
    }

    // Allowed projects/apps
    if let Some(projects) = node.allowed_projects {
        o_detail!("  Allowed Projects: {}", projects.join(", ").cyan());
    } else {
        o_detail!("  Allowed Projects: {}", "all".dimmed());
    }

    if let Some(apps) = node.allowed_apps {
        o_detail!("  Allowed Apps: {}", apps.join(", ").cyan());
    } else {
        o_detail!("  Allowed Apps: {}", "all".dimmed());
    }

    // Bound apps
    if let Some(bound_apps) = node.bound_apps {
        if !bound_apps.is_empty() {
            o_detail!();
            o_step!("{}", "Bound Apps:".bold());
            for app in bound_apps {
                let primary = if app.is_primary.unwrap_or(0) > 0 {
                    " (primary)".green()
                } else {
                    "".normal()
                };
                o_detail!("  • {}.{}{}", app.name.cyan(), app.project_name, primary);
            }
        }
    }

    o_detail!();
    o_step!("{}", "Commands:".yellow());
    o_detail!("  SSH:    ops ssh {}", node_id);
    o_detail!("  Ping:   ops ping {}", node_id);
    o_detail!("  Delete: ops node remove {}", node_id);

    Ok(())
}

/// Remove a node
pub async fn handle_remove(node_id: u64, force: bool, interactive: bool) -> Result<()> {
    let cfg = config::load_config()
        .context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token
        .context("You are not logged in. Please run `ops login` first.")?;

    if !force {
        o_warn!("{}", format!("This will delete node #{} and all its associated data.", node_id).yellow());
        o_detail!("The node's DNS record will also be removed.");
        o_detail!();

        if interactive {
            if !prompt::confirm_no("Are you sure?", interactive)? {
                o_warn!("Aborted.");
                return Ok(());
            }
        } else {
            // Non-interactive without --force: refuse to delete
            return Err(anyhow!("Destructive operation requires --force in non-interactive mode"));
        }
    }

    o_step!("Deleting node #{}...", node_id);

    let res = api::delete_node(&token, node_id).await?;

    o_success!("{}", format!("✔ {}", res.message).green());

    Ok(())
}
