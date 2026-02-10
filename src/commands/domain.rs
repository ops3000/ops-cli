use anyhow::{bail, Context, Result};
use colored::Colorize;
use crate::{api, config};
use crate::commands::deploy::load_ops_toml;
use crate::types::OpsToml;

/// Resolve (project, app) from ops.toml + optional --app flag.
/// - Legacy mode (top-level `app`): use app as both project and app name.
/// - Project mode: project from `project` field; app from --app flag or auto-detect
///   if there's exactly one entry in [[apps]].
fn resolve_project_app(ops_config: &OpsToml, app_flag: Option<&str>) -> Result<(String, String)> {
    // Legacy mode: top-level `app` field
    if let Some(ref app) = ops_config.app {
        let project = ops_config.project.as_ref().unwrap_or(app);
        return Ok((project.clone(), app.clone()));
    }

    // Project mode
    let project = ops_config.project.as_ref()
        .context("ops.toml must have 'project' or 'app'")?;

    let app_name = if let Some(name) = app_flag {
        // --app flag: verify it exists in [[apps]]
        if !ops_config.apps.is_empty()
            && !ops_config.apps.iter().any(|a| a.name == name)
        {
            bail!("App '{}' not found in [[apps]]. Available: {}",
                name,
                ops_config.apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "));
        }
        name.to_string()
    } else if ops_config.apps.len() == 1 {
        // Auto-detect: exactly one app
        ops_config.apps[0].name.clone()
    } else if ops_config.apps.is_empty() {
        bail!("No [[apps]] defined in ops.toml. Use --app to specify the app name.");
    } else {
        bail!("Multiple apps in ops.toml. Use --app to specify which one.\nAvailable: {}",
            ops_config.apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "));
    };

    Ok((project.clone(), app_name))
}

pub async fn handle_add(file: String, domain: String, app_flag: Option<String>) -> Result<()> {
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    let ops_config = load_ops_toml(&file)?;
    let (project, app) = resolve_project_app(&ops_config, app_flag.as_deref())?;

    o_step!("{} Adding domain {}...", "🌐".cyan(), domain.green());

    let resp = api::add_custom_domain(&token, &project, &app, &domain).await?;

    o_success!("\n{} {}", "✔".green(), resp.message);
    o_detail!("  CNAME: {} → {}", domain.cyan(), resp.cname_target.green());
    o_detail!("  SSL:   {}", resp.ssl_status);

    if let Some(ref url) = resp.domain_connect_url {
        o_result!();
        o_result!("  {} {}", "Auto-configure DNS:".cyan().bold(), url);
    }

    o_warn!("\n{}", resp.instructions.yellow());

    Ok(())
}

pub async fn handle_list(file: String, app_flag: Option<String>) -> Result<()> {
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    let ops_config = load_ops_toml(&file)?;
    let (project, app) = resolve_project_app(&ops_config, app_flag.as_deref())?;

    let resp = api::list_custom_domains(&token, &project, &app).await?;

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
