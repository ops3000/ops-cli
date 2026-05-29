use crate::{api, config};
use anyhow::{Context, Result};
use colored::Colorize;

/// List all orgs the current user is a member of
pub async fn handle_list() -> Result<()> {
    let cfg = config::load_config()
        .context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token
        .context("You are not logged in. Please run `ops login` first.")?;

    let res = api::list_orgs(&token).await?;

    if res.orgs.is_empty() {
        o_warn!("{}", "No orgs found.".yellow());
        return Ok(());
    }

    o_step!("{}", "My Orgs:".bold());
    o_detail!();

    for org in res.orgs {
        let personal_tag = if org.is_personal {
            " (personal)".dimmed()
        } else {
            "".normal()
        };
        o_detail!(
            "  {} {}{}  role: {}",
            format!("#{}", org.id).dimmed(),
            org.slug.cyan().bold(),
            personal_tag,
            org.role.yellow()
        );
        o_detail!("      Name: {}", org.name);
    }

    o_detail!();
    o_detail!("{}", "Use OPS_ORG=<slug> or X-Ops-Org header to scope commands.".dimmed());

    Ok(())
}
