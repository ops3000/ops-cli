use crate::{api, config};
use anyhow::{Context, Result};
use colored::Colorize;

pub async fn handle_create_project(name: String) -> Result<()> {
    println!("Creating project '{}'...", name.cyan());
    let cfg = config::load_config().context("Config not found. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    let res = api::create_project(&token, &name).await?;
    println!("{}", format!("✔ {}", res.message).green());
    Ok(())
}