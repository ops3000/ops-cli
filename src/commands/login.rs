use crate::{api, config};
use anyhow::{Context, Result};
use colored::Colorize;
use std::io::{self, Write};

pub async fn handle_login() -> Result<()> {
    print!("Enter username: ");
    io::stdout().flush()?;
    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    
    let password = rpassword::prompt_password("Enter password: ")?;
    
    println!("Logging in...");
    let res = api::login(username.trim(), &password).await?;
    
    let mut cfg = config::load_config().unwrap_or_default();
    cfg.token = Some(res.token);
    config::save_config(&cfg).context("Failed to save credentials")?;

    println!("{}", "✔ Login successful! Token saved.".green());
    Ok(())
}