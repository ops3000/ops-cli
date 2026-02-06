use crate::commands::deploy::load_ops_toml;
use crate::commands::ssh;
use anyhow::{Context, Result};
use colored::Colorize;

pub async fn handle_status(file: String) -> Result<()> {
    let config = load_ops_toml(&file)?;

    let target = config.target.as_deref()
        .context("ops.toml must have 'target' for status command")?;

    println!("{} {}\n", "📊 Status:".cyan(), target.green());

    let cmd = format!(
        "cd {} && docker compose ps",
        config.deploy_path
    );

    ssh::execute_remote_command(target, &cmd, None).await?;
    Ok(())
}
