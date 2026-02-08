use anyhow::{Context, Result};
use colored::Colorize;
use crate::{api, config};
use crate::commands::deploy::load_ops_toml;

pub async fn handle_add(file: String, domain: String) -> Result<()> {
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    let ops_config = load_ops_toml(&file)?;
    let project = ops_config.project.as_ref()
        .or(ops_config.app.as_ref())
        .context("ops.toml must have 'project' or 'app'")?;
    let app = ops_config.app.as_ref()
        .or(ops_config.project.as_ref())
        .context("ops.toml must have 'app' or 'project'")?;

    o_step!("{} Adding domain {}...", "🌐".cyan(), domain.green());

    let resp = api::add_custom_domain(&token, project, app, &domain).await?;

    o_success!("\n{} {}", "✔".green(), resp.message);
    o_detail!("  CNAME: {} → {}", domain.cyan(), resp.cname_target.green());
    o_detail!("  SSL:   {}", resp.ssl_status);
    o_warn!("\n{}", resp.instructions.yellow());

    Ok(())
}

pub async fn handle_list(file: String) -> Result<()> {
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    let ops_config = load_ops_toml(&file)?;
    let project = ops_config.project.as_ref()
        .or(ops_config.app.as_ref())
        .context("ops.toml must have 'project' or 'app'")?;
    let app = ops_config.app.as_ref()
        .or(ops_config.project.as_ref())
        .context("ops.toml must have 'app' or 'project'")?;

    let resp = api::list_custom_domains(&token, project, app).await?;

    o_step!("{} Domains for {}.{}:\n", "🌐".cyan(), app.green(), project.green());
    o_detail!("  {} (default)", resp.default_domain.cyan());

    if resp.domains.is_empty() {
        o_detail!("\n  No custom domains configured.");
    } else {
        for d in &resp.domains {
            let status_color = match d.status.as_str() {
                "active" => d.status.green(),
                "pending" => d.status.yellow(),
                _ => d.status.red(),
            };
            o_detail!("  {} [{}]", d.domain.cyan(), status_color);
        }
    }

    Ok(())
}

pub async fn handle_remove(file: String, domain: String) -> Result<()> {
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    // Just need to verify we're logged in; the backend handles ownership
    let _ = load_ops_toml(&file)?;

    o_step!("{} Removing domain {}...", "🌐".cyan(), domain.yellow());

    let resp = api::remove_custom_domain(&token, &domain).await?;
    o_success!("{} {}", "✔".green(), resp.message);

    Ok(())
}
