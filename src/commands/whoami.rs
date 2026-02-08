use crate::{api, config};
use anyhow::{Context, Result};
use colored::Colorize;

pub async fn handle_whoami() -> Result<()> {
    let cfg = config::load_config().context("Could not load config. Are you logged in?")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;
    
    let res = api::whoami(&token).await?;

    o_result!("You are logged in as:");
    o_detail!("  {} {}", "User ID:".bold(), res.user_id);
    o_detail!("  {}   {}", "Username:".bold(), res.username.cyan());
    o_detail!("  {} {}", "Token Expires:".bold(), res.token_expires_at);

    Ok(())
}