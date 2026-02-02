use crate::commands::deploy::load_ops_toml;
use crate::commands::ssh;
use anyhow::Result;
use colored::Colorize;

pub async fn handle_status(file: String) -> Result<()> {
    let config = load_ops_toml(&file)?;

    println!("{} {}\n", "📊 Status:".cyan(), config.target.green());

    let cmd = format!(
        "cd {} && docker compose ps",
        config.deploy_path
    );

    ssh::execute_remote_command(&config.target, &cmd, None).await?;
    Ok(())
}
