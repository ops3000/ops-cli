use crate::api;
use anyhow::{anyhow, Result};
use colored::Colorize;
use std::io::{self, Write};

pub async fn handle_register() -> Result<()> {
    o_print!("Enter new username: ");
    io::stdout().flush()?;
    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    
    let password = rpassword::prompt_password("Enter a strong password: ")?;
    let password_confirm = rpassword::prompt_password("Confirm password: ")?;

    if password != password_confirm {
        return Err(anyhow!("Passwords do not match."));
    }
    
    if password.len() < 8 {
        return Err(anyhow!("Password must be at least 8 characters long."));
    }
    
    o_step!("Registering new user...");
    let res = api::register(username.trim(), &password).await?;
    
    o_success!("{}", format!("✔ {}", res.message).green());
    o_detail!("You can now log in with `ops login`.");
    Ok(())
}