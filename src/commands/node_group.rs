use crate::{api, config, utils};
use anyhow::{Context, Result};
use colored::Colorize;

/// Create a new node group
pub async fn handle_create(
    project: String,
    env: String,
    name: Option<String>,
    strategy: String,
) -> Result<()> {
    let cfg = config::load_config().context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    println!("Creating node group...");
    println!("  Project:     {}", project.cyan());
    println!("  Environment: {}", env.cyan());
    if let Some(ref n) = name {
        println!("  Name:        {}", n.cyan());
    }
    println!("  Strategy:    {}", strategy.cyan());

    let res = api::create_node_group(&token, &project, &env, name.as_deref(), &strategy).await?;

    println!("{}", format!("✔ {}", res.message).green());
    println!();
    println!("Node Group Details:");
    println!("  ID:          {}", res.node_group.id);
    println!("  Name:        {}", res.node_group.name);
    println!("  Environment: {}", res.node_group.environment);
    println!("  Strategy:    {}", res.node_group.lb_strategy);

    println!();
    println!("{}", "Next steps:".yellow());
    println!("  1. SSH into your server(s)");
    println!("  2. Run: ops set {}.{} --region <region>", env, project);

    Ok(())
}

/// List node groups for a project
pub async fn handle_list(project: Option<String>) -> Result<()> {
    let cfg = config::load_config().context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    let res = api::list_node_groups(&token, project.as_deref()).await?;

    if res.node_groups.is_empty() {
        println!("{}", "No node groups found.".yellow());
        println!();
        println!("Create one with: ops node-group create --project <name> --env <environment>");
        return Ok(());
    }

    println!("{}", "Node Groups:".bold());
    println!();

    for group in res.node_groups {
        let project_name = group.project_name.unwrap_or_else(|| "-".to_string());
        let node_count = group.node_count.unwrap_or(0);
        let healthy_count = group.healthy_count.unwrap_or(0);

        // Status indicator
        let status = if node_count == 0 {
            "○".dimmed()
        } else if healthy_count == node_count {
            "●".green()
        } else if healthy_count > 0 {
            "◐".yellow()
        } else {
            "●".red()
        };

        println!(
            "  {} {} {} ({}) - {} nodes ({} healthy) [{}]",
            status,
            format!("#{}", group.id).dimmed(),
            group.name.cyan().bold(),
            project_name,
            node_count,
            healthy_count,
            group.lb_strategy.dimmed()
        );
    }

    println!();
    println!("{}", "Use 'ops node-group show <id>' for details".dimmed());

    Ok(())
}

/// Show node group details including member nodes
pub async fn handle_show(id: i64) -> Result<()> {
    let cfg = config::load_config().context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    let group = api::get_node_group(&token, id).await?;

    println!("{}", format!("Node Group: {}", group.name).bold());
    println!();
    println!("  ID:          {}", group.id);
    println!("  Project:     {}", group.project_name);
    println!("  Environment: {}", group.environment);
    println!("  Strategy:    {}", group.lb_strategy);

    // Health check config
    if let Some(hc) = group.health_config {
        println!();
        println!("{}", "Health Check Config:".bold());
        println!("  Type:      {}", hc.check_type);
        println!("  Endpoint:  {}", hc.endpoint);
        println!("  Interval:  {}s", hc.interval_seconds);
        println!("  Timeout:   {}s", hc.timeout_seconds);
        println!("  Thresholds: {} unhealthy / {} healthy", hc.unhealthy_threshold, hc.healthy_threshold);
    }

    println!();
    println!("{}", format!("Nodes ({}):", group.nodes.len()).bold());

    if group.nodes.is_empty() {
        println!("  {}", "No nodes in this group.".dimmed());
        println!();
        println!("{}", "Add nodes by running on your server:".yellow());
        println!("  ops set {}.{} --region <region>", group.environment, group.project_name);
    } else {
        println!();
        for node in group.nodes {
            let status_icon = match node.status.as_str() {
                "healthy" => "●".green(),
                "unhealthy" => "●".red(),
                "draining" => "◐".yellow(),
                _ => "○".dimmed(),
            };

            let region_str = node.region.as_deref().unwrap_or("-");
            let zone_str = node.zone.as_deref().unwrap_or("-");
            let hostname_str = node.hostname.as_deref().unwrap_or(&node.ip_address);
            let serve_status = if node.has_serve_token.unwrap_or(0) > 0 {
                "serve ✓".green()
            } else {
                "serve ✗".red()
            };

            println!(
                "  {} {} ({}) - {} zone:{} weight:{} [{}]",
                status_icon,
                hostname_str.cyan(),
                node.ip_address,
                region_str,
                zone_str,
                node.weight,
                serve_status
            );
            println!("      Domain: {}", node.domain.dimmed());
            if let Some(last_check) = node.last_health_check {
                println!("      Last check: {}", last_check.dimmed());
            }
        }
    }

    Ok(())
}

/// List nodes in a specific environment
pub async fn handle_nodes(target_str: String) -> Result<()> {
    let target = utils::parse_target(&target_str)?;

    let cfg = config::load_config().context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    let res = api::get_nodes_in_env(&token, &target.project, &target.environment).await?;

    println!(
        "{} ({}.{}.ops.autos)",
        "Node Group".bold(),
        target.environment.cyan(),
        target.project.cyan()
    );
    println!();
    println!("  Name:     {}", res.node_group.name);
    println!("  Strategy: {}", res.node_group.lb_strategy);
    println!();

    if res.nodes.is_empty() {
        println!("{}", "No nodes found.".yellow());
        return Ok(());
    }

    println!("{}", format!("Nodes ({}):", res.nodes.len()).bold());
    println!();

    for node in res.nodes {
        let status_icon = match node.status.as_str() {
            "healthy" => "●".green(),
            "unhealthy" => "●".red(),
            "draining" => "◐".yellow(),
            _ => "○".dimmed(),
        };

        let region_str = node.region.as_deref().unwrap_or("-");
        let hostname_str = node.hostname.as_deref().unwrap_or(&node.ip_address);

        println!(
            "  {} {} ({}) region:{} weight:{}",
            status_icon,
            hostname_str.cyan(),
            node.ip_address,
            region_str,
            node.weight
        );
    }

    Ok(())
}
