use crate::{api, config, ssh};
use anyhow::{Context, Result};
use colored::Colorize;

pub async fn handle_set(project: String, environment: String) -> Result<()> {
    let cfg = config::load_config().context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;
    
    println!("Fetching your public SSH key from ~/.ssh/id_rsa.pub...");
    let pubkey = ssh::get_default_pubkey()?;
    println!("{}", "✔ Found public key.".green());

    println!("Binding this server to project '{}' and environment '{}'...", project.cyan(), environment.cyan());
    let res = api::set_node(&token, &project, &environment, &pubkey).await?;

    println!("{}", format!("✔ {}", res.message).green());

    println!("Adding CI public key to ~/.ssh/authorized_keys for automated deployments...");
    ssh::add_to_authorized_keys(&res.ci_ssh_public_key)?;
    println!("{}", "✔ CI key added successfully.".green());
    println!("\n{}", "Setup complete! Your CI/CD pipeline can now deploy to this server.".bold());
    
    Ok(())
}