use crate::{api, config, ssh, utils};
use anyhow::{Context, Result};
use colored::Colorize;
use std::io::{self, Write};

fn prompt_confirm(prompt: &str) -> Result<bool> {
    print!("{} [y/N]: ", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();
    Ok(input == "y" || input == "yes")
}

/// Handle `ops set` command
/// Two modes:
/// 1. Local binding: `ops set api.RedQ` - bind this server to the app (legacy mode)
/// 2. Remote binding: `ops set api.RedQ --node 12345` - bind a specific node to the app
pub async fn handle_set(
    target_str: String,
    node_id: Option<u64>,
    primary: bool,
    region: Option<String>,
    zone: Option<String>,
    hostname: Option<String>,
    weight: Option<u8>,
) -> Result<()> {
    let target = utils::parse_target(&target_str)?;

    let cfg = config::load_config().context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    // If --node is provided, use the new binding flow
    if let Some(nid) = node_id {
        return handle_remote_bind(&token, &target, nid, primary, weight).await;
    }

    // Legacy local binding flow
    println!("You are about to bind this server to:");
    println!("  Project:     {}", target.project.cyan().bold());
    println!("  Environment: {}", target.environment.cyan().bold());
    if let Some(ref r) = region {
        println!("  Region:      {}", r.cyan());
    }
    if let Some(ref z) = zone {
        println!("  Zone:        {}", z.cyan());
    }
    if let Some(ref h) = hostname {
        println!("  Hostname:    {}", h.cyan());
    }
    println!("  Full Domain: {}.{}.ops.autos", target.environment, target.project);
    println!();

    if !prompt_confirm("Do you want to continue?")? {
        println!("Operation cancelled.");
        return Ok(());
    }

    let mut force_reset = false;
    println!();
    println!("{}", "Tip: If you are having trouble with 'ops ssh' (invalid key format), please choose Yes below.".dimmed());
    if prompt_confirm("Do you want to regenerate CI/CD SSH keys for this environment?")? {
        force_reset = true;
    }

    println!("\nChecking local SSH key...");
    let pubkey = ssh::get_default_pubkey()?;
    println!("{}", "✔ SSH key ready.".green());

    println!("Binding server...");

    let res = api::set_node(
        &token,
        &target.project,
        &target.environment,
        &pubkey,
        force_reset,
        region.as_deref(),
        zone.as_deref(),
        hostname.as_deref(),
        weight,
    ).await?;

    println!("{}", format!("✔ {}", res.message).green());
    println!("  Node ID:       {}", res.node_id);
    println!("  Node Group ID: {}", res.node_group_id);
    println!("  Domain:        {}", res.domain.cyan());
    if let Some(ref r) = res.region {
        println!("  Region:        {}", r);
    }

    println!("Adding CI public key to ~/.ssh/authorized_keys...");
    ssh::add_to_authorized_keys(&res.ci_ssh_public_key)?;

    if force_reset {
        println!("{}", "✔ CI keys have been regenerated. Please try `ops ssh` again.".green());
    }

    println!("{}", "✔ Setup complete!".green());
    Ok(())
}

/// Handle remote node binding (--node flag provided)
/// This binds an existing node to an app
/// Target format: app.project (e.g., api.RedQ)
async fn handle_remote_bind(
    token: &str,
    target: &utils::Target,
    node_id: u64,
    primary: bool,
    weight: Option<u8>,
) -> Result<()> {
    // Note: In the new model, target.environment is actually the app name
    // and target.project is the project name
    let app_name = &target.environment;
    let project_name = &target.project;

    println!("Binding node #{} to {}.{}...",
        node_id.to_string().cyan(),
        app_name.cyan(),
        project_name.cyan()
    );

    let result = api::bind_node_by_name(
        token,
        project_name,
        app_name,
        node_id,
        primary,
        weight,
    ).await?;

    println!("{}", format!("✔ {}", result.message).green());
    println!("  App ID:     {}", result.app_id);
    println!("  Mode:       {}", result.mode);
    println!("  Domain:     {}", result.domain.cyan());

    if let Some(group_id) = result.node_group_id {
        println!("  Node Group: #{}", group_id);
    }
    if let Some(total) = result.total_nodes {
        println!("  Total Nodes: {}", total);
    }

    Ok(())
}