// src/commands/token.rs
use crate::config;
use anyhow::{Context, Result};

pub async fn handle_get_token() -> Result<()> {
    let cfg = config::load_config().context("Could not load config. Are you logged in?")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;
    print!("{}", token); // 直接打印，不带换行，方便脚本捕获
    Ok(())
}