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

    o_step!("Creating node group...");
    o_detail!("  Project:     {}", project.cyan());
    o_detail!("  Environment: {}", env.cyan());
    if let Some(ref n) = name {
        o_detail!("  Name:        {}", n.cyan());
    }
    o_detail!("  Strategy:    {}", strategy.cyan());

    let res = api::create_node_group(&token, &project, &env, name.as_deref(), &strategy).await?;

    o_success!("{}", format!("✔ {}", res.message).green());
    o_detail!();
    o_step!("Node Group Details:");
    o_detail!("  ID:          {}", res.node_group.id);
    o_detail!("  Name:        {}", res.node_group.name);
    o_detail!("  Environment: {}", res.node_group.environment);
    o_detail!("  Strategy:    {}", res.node_group.lb_strategy);

    o_detail!();
    o_step!("{}", "Next steps:".yellow());
    o_detail!("  1. SSH into your server(s)");
    o_detail!("  2. Run: ops set {}.{} --region <region>", env, project);

    Ok(())
}

/// List node groups for a project
pub async fn handle_list(project: Option<String>) -> Result<()> {
    let cfg = config::load_config().context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    let res = api::list_node_groups(&token, project.as_deref()).await?;

    if res.node_groups.is_empty() {
        o_warn!("{}", "No node groups found.".yellow());
        o_detail!();
        o_detail!("Create one with: ops node-group create --project <name> --env <environment>");
        return Ok(());
    }

    o_step!("{}", "Node Groups:".bold());
    o_detail!();

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

        o_detail!(
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

    o_detail!();
    o_detail!("{}", "Use 'ops node-group show <id>' for details".dimmed());

    Ok(())
}

/// Show node group details including member nodes
pub async fn handle_show(id: i64) -> Result<()> {
    let cfg = config::load_config().context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    let group = api::get_node_group(&token, id).await?;

    o_step!("{}", format!("Node Group: {}", group.name).bold());
    o_detail!();
    o_detail!("  ID:          {}", group.id);
    o_detail!("  Project:     {}", group.project_name);
    o_detail!("  Environment: {}", group.environment);
    o_detail!("  Strategy:    {}", group.lb_strategy);

    // Health check config
    if let Some(hc) = group.health_config {
        o_detail!();
        o_step!("{}", "Health Check Config:".bold());
        o_detail!("  Type:      {}", hc.check_type);
        o_detail!("  Endpoint:  {}", hc.endpoint);
        o_detail!("  Interval:  {}s", hc.interval_seconds);
        o_detail!("  Timeout:   {}s", hc.timeout_seconds);
        o_detail!("  Thresholds: {} unhealthy / {} healthy", hc.unhealthy_threshold, hc.healthy_threshold);
    }

    o_detail!();
    o_step!("{}", format!("Nodes ({}):", group.nodes.len()).bold());

    if group.nodes.is_empty() {
        o_detail!("  {}", "No nodes in this group.".dimmed());
        o_detail!();
        o_step!("{}", "Add nodes by running on your server:".yellow());
        o_detail!("  ops set {}.{} --region <region>", group.environment, group.project_name);
    } else {
        o_detail!();
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

            o_detail!(
                "  {} {} ({}) - {} zone:{} weight:{} [{}]",
                status_icon,
                hostname_str.cyan(),
                node.ip_address,
                region_str,
                zone_str,
                node.weight,
                serve_status
            );
            o_detail!("      Domain: {}", node.domain.dimmed());
            if let Some(last_check) = node.last_health_check {
                o_detail!("      Last check: {}", last_check.dimmed());
            }
        }
    }

    Ok(())
}

/// List nodes in a specific environment
pub async fn handle_nodes(target_str: String) -> Result<()> {
    let target = utils::parse_target(&target_str)?;

    let (app, project) = match &target {
        utils::Target::AppTarget { app, project, .. } => (app.clone(), project.clone()),
        utils::Target::NodeId { .. } => {
            anyhow::bail!("Expected app.project format (e.g., api.RedQ), not a node ID");
        }
    };

    let cfg = config::load_config().context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    let res = api::get_nodes_in_env(&token, &project, &app).await?;

    o_step!(
        "{} ({}.{}.ops.autos)",
        "Node Group".bold(),
        app.cyan(),
        project.cyan()
    );
    o_detail!();
    o_detail!("  Name:     {}", res.node_group.name);
    o_detail!("  Strategy: {}", res.node_group.lb_strategy);
    o_detail!();

    if res.nodes.is_empty() {
        o_warn!("{}", "No nodes found.".yellow());
        return Ok(());
    }

    o_step!("{}", format!("Nodes ({}):", res.nodes.len()).bold());
    o_detail!();

    for node in res.nodes {
        let status_icon = match node.status.as_str() {
            "healthy" => "●".green(),
            "unhealthy" => "●".red(),
            "draining" => "◐".yellow(),
            _ => "○".dimmed(),
        };

        let region_str = node.region.as_deref().unwrap_or("-");
        let hostname_str = node.hostname.as_deref().unwrap_or(&node.ip_address);

        o_detail!(
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
