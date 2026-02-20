use crate::{api, config, prompt, ssh, utils};
use anyhow::{Context, Result};
use colored::Colorize;

/// Handle `ops set` command
/// Two modes:
/// 1. Local binding: `ops set api.RedQ` - bind this server to the app (requires app.project format)
/// 2. Remote binding: `ops set api.RedQ --node 12345` - bind a specific node to the app
pub async fn handle_set(
    target_str: String,
    node_id: Option<u64>,
    primary: bool,
    region: Option<String>,
    zone: Option<String>,
    hostname: Option<String>,
    weight: Option<u8>,
    interactive: bool,
) -> Result<()> {
    let target = utils::parse_target(&target_str)?;

    let (app_name, project_name) = match &target {
        utils::Target::AppTarget { app, project, .. } => (app.clone(), project.clone()),
        utils::Target::NodeId { .. } => {
            anyhow::bail!("ops set requires app.project format (e.g., api.RedQ), not a node ID");
        }
    };

    let cfg = config::load_config().context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    // If --node is provided, use the remote binding flow
    if let Some(nid) = node_id {
        o_step!("Binding node #{} to {}.{}...",
            nid.to_string().cyan(),
            app_name.cyan(),
            project_name.cyan()
        );

        let result = api::bind_node_by_name(
            &token,
            &project_name,
            &app_name,
            nid,
            primary,
            weight,
        ).await?;

        o_success!("{}", format!("✔ {}", result.message).green());
        o_detail!("  App ID:     {}", result.app_id);
        o_detail!("  Mode:       {}", result.mode);
        o_detail!("  Domain:     {}", result.domain.cyan());

        if let Some(group_id) = result.node_group_id {
            o_detail!("  Node Group: #{}", group_id);
        }
        if let Some(total) = result.total_nodes {
            o_detail!("  Total Nodes: {}", total);
        }

        return Ok(());
    }

    // Legacy local binding flow
    o_step!("You are about to bind this server to:");
    o_detail!("  Project:     {}", project_name.cyan().bold());
    o_detail!("  App:         {}", app_name.cyan().bold());
    if let Some(ref r) = region {
        o_detail!("  Region:      {}", r.cyan());
    }
    if let Some(ref z) = zone {
        o_detail!("  Zone:        {}", z.cyan());
    }
    if let Some(ref h) = hostname {
        o_detail!("  Hostname:    {}", h.cyan());
    }
    o_detail!("  Full Domain: {}.{}.ops.autos", app_name, project_name);
    o_detail!();

    if !prompt::confirm_yes("Do you want to continue?", interactive)? {
        o_warn!("Operation cancelled.");
        return Ok(());
    }

    let mut force_reset = false;
    o_detail!();
    o_detail!("{}", "Tip: If you are having trouble with 'ops ssh' (invalid key format), please choose Yes below.".dimmed());
    if prompt::confirm_no("Do you want to regenerate CI/CD SSH keys for this environment?", interactive)? {
        force_reset = true;
    }

    o_step!("\nChecking local SSH key...");
    let pubkey = ssh::get_default_pubkey()?;
    o_success!("{}", "✔ SSH key ready.".green());

    o_step!("Binding server...");

    let res = api::set_node(
        &token,
        &project_name,
        &app_name,
        &pubkey,
        force_reset,
        region.as_deref(),
        zone.as_deref(),
        hostname.as_deref(),
        weight,
    ).await?;

    o_success!("{}", format!("✔ {}", res.message).green());
    o_detail!("  Node ID:       {}", res.node_id);
    o_detail!("  Node Group ID: {}", res.node_group_id);
    o_detail!("  Domain:        {}", res.domain.cyan());
    if let Some(ref r) = res.region {
        o_detail!("  Region:        {}", r);
    }

    o_step!("Adding CI public key to ~/.ssh/authorized_keys...");
    ssh::add_to_authorized_keys(&res.ci_ssh_public_key)?;

    if force_reset {
        o_success!("{}", "✔ CI keys have been regenerated. Please try `ops ssh` again.".green());
    }

    o_success!("{}", "✔ Setup complete!".green());
    Ok(())
}
