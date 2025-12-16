use crate::{api, config};
use anyhow::{Context, Result};
use colored::Colorize;

pub async fn handle_whoami() -> Result<()> {
    let cfg = config::load_config().context("Could not load config. Are you logged in?")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;
    
    let res = api::whoami(&token).await?;

    println!("You are logged in as:");
    println!("  {} {}", "User ID:".bold(), res.user_id);
    println!("  {}   {}", "Username:".bold(), res.username.cyan());
    println!("  {} {}", "Token Expires:".bold(), res.token_expires_at);

    Ok(())
}