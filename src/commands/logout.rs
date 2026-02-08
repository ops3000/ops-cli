// src/commands/logout.rs
use crate::config;
use anyhow::{Context, Result};
use colored::Colorize;

pub async fn handle_logout() -> Result<()> {
    let mut cfg = config::load_config().context("Could not load config file.")?;

    if cfg.token.is_none() {
        o_warn!("{}", "You are not logged in.".yellow());
        return Ok(());
    }

    cfg.token = None;
    config::save_config(&cfg).context("Failed to clear credentials.")?;

    o_success!("{}", "✔ You have been logged out.".green());
    Ok(())
}