use std::collections::HashSet;
use std::io::{self, Write};
use anyhow::{bail, Context, Result};
use colored::Colorize;
use crate::{api, config};
use crate::commands::deploy::load_ops_toml;
use crate::types::OpsToml;

/// Resolve (project, app) from ops.toml + optional --app flag.
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
        if !ops_config.apps.is_empty()
            && !ops_config.apps.iter().any(|a| a.name == name)
        {
            bail!("App '{}' not found in [[apps]]. Available: {}",
                name,
                ops_config.apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "));
        }
        name.to_string()
    } else if ops_config.apps.len() == 1 {
        ops_config.apps[0].name.clone()
    } else if ops_config.apps.is_empty() {
        bail!("No [[apps]] defined in ops.toml. Use --app to specify the app name.");
    } else {
        bail!("Multiple apps in ops.toml. Use --app to specify which one.\nAvailable: {}",
            ops_config.apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "));
    };

    Ok((project.clone(), app_name))
}

/// Build sync targets: Vec<(project, app_name, desired_domains)>
fn build_sync_targets(ops_config: &OpsToml, app_flag: Option<&str>) -> Result<Vec<(String, String, Vec<String>)>> {
    let mut targets = Vec::new();

    // Legacy mode
    if let Some(ref app) = ops_config.app {
        let project = ops_config.project.as_ref().unwrap_or(app);
        if !ops_config.domains.is_empty() {
            targets.push((project.clone(), app.clone(), ops_config.domains.clone()));
        }
        return Ok(targets);
    }

    // Project mode
    let project = ops_config.project.as_ref()
        .context("ops.toml must have 'project' or 'app'")?;

    if let Some(name) = app_flag {
        let app_def = ops_config.apps.iter().find(|a| a.name == name)
            .with_context(|| format!(
                "App '{}' not found in [[apps]]. Available: {}",
                name,
                ops_config.apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
            ))?;
        if !app_def.domains.is_empty() {
            targets.push((project.clone(), app_def.name.clone(), app_def.domains.clone()));
        }
    } else {
        for app_def in &ops_config.apps {
            if !app_def.domains.is_empty() {
                targets.push((project.clone(), app_def.name.clone(), app_def.domains.clone()));
            }
        }
    }

    Ok(targets)
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

    let _ = load_ops_toml(&file)?;

    o_step!("{} Removing domain {}...", "🌐".cyan(), domain.yellow());

    let resp = api::remove_custom_domain(&token, &domain).await?;
    o_success!("{} {}", "✔".green(), resp.message);

    Ok(())
}

pub async fn handle_sync(file: String, app_flag: Option<String>, prune: bool, yes: bool) -> Result<()> {
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;
    let ops_config = load_ops_toml(&file)?;

    let sync_targets = build_sync_targets(&ops_config, app_flag.as_deref())?;

    if sync_targets.is_empty() {
        o_warn!("No domains declared in ops.toml. Nothing to sync.");
        return Ok(());
    }

    let mut total_added: u32 = 0;
    let mut total_removed: u32 = 0;
    let mut total_errors: u32 = 0;

    for (project, app_name, desired) in &sync_targets {
        o_step!("\n{} Syncing domains for {}.{}...", "🌐".cyan(), app_name.green(), project.green());

        let desired_set: HashSet<&str> = desired.iter().map(|s| s.as_str()).collect();

        // Fetch existing domains from backend
        let existing_resp = api::list_custom_domains(&token, project, app_name).await?;
        let existing_set: HashSet<String> = existing_resp.domains.iter()
            .map(|d| d.domain.clone()).collect();

        let to_add: Vec<&str> = desired_set.iter()
            .filter(|d| !existing_set.contains(**d))
            .copied().collect();
        let to_remove: Vec<&String> = existing_set.iter()
            .filter(|d| !desired_set.contains(d.as_str()))
            .collect();

        if to_add.is_empty() && to_remove.is_empty() {
            o_success!("   {} Already in sync ({} domain(s))", "✔".green(), desired.len());
            continue;
        }

        // Add missing domains
        for domain in &to_add {
            match api::add_custom_domain(&token, project, app_name, domain).await {
                Ok(resp) => {
                    o_success!("   {} Added {} → CNAME {}", "+".green(), domain.cyan(), resp.cname_target.green());
                    if let Some(ref url) = resp.domain_connect_url {
                        o_detail!("     DNS: {}", url);
                    }
                    total_added += 1;
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("already exists") || msg.contains("UNIQUE") {
                        o_detail!("   {} {} (already exists)", "=".dimmed(), domain);
                    } else {
                        o_error!("   {} Failed to add {}: {}", "✘".red(), domain, msg);
                        total_errors += 1;
                    }
                }
            }
        }

        // Handle extra domains in backend
        if !to_remove.is_empty() {
            if prune {
                if !yes {
                    o_warn!("\n   Domains to remove from backend:");
                    for d in &to_remove {
                        o_warn!("     - {}", d);
                    }
                    print!("   Continue? [y/N]: ");
                    io::stdout().flush()?;
                    let mut input = String::new();
                    io::stdin().read_line(&mut input)?;
                    if input.trim().to_lowercase() != "y" {
                        o_warn!("   Skipped pruning for {}.{}", app_name, project);
                        continue;
                    }
                }
                for domain in &to_remove {
                    match api::remove_custom_domain(&token, domain).await {
                        Ok(_) => {
                            o_success!("   {} Removed {}", "-".red(), domain.yellow());
                            total_removed += 1;
                        }
                        Err(e) => {
                            o_error!("   {} Failed to remove {}: {}", "✘".red(), domain, e);
                            total_errors += 1;
                        }
                    }
                }
            } else {
                o_warn!("   {} {} domain(s) in backend not in ops.toml:", "⚠".yellow(), to_remove.len());
                for d in &to_remove {
                    o_warn!("     - {}", d);
                }
                o_warn!("   Use --prune to remove them.");
            }
        }
    }

    // Summary
    o_result!("\n{} Domain sync complete: {} added, {} removed, {} errors",
        "✔".green(), total_added, total_removed, total_errors);

    if total_errors > 0 {
        bail!("{} domain operation(s) failed", total_errors);
    }

    Ok(())
}
