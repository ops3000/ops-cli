use crate::{api, config, ssh, utils};
use anyhow::{Context, Result};
use colored::Colorize;
use std::io::{self, Write};

fn prompt_confirm(prompt: &str) -> Result<bool> {
    print!("{} [y/N]: ", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();
    Ok(input == "y" || input == "yes")
}

pub async fn handle_set(target_str: String) -> Result<()> {
    let target = utils::parse_target(&target_str)?;
    
    let cfg = config::load_config().context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;
    
    // 1. 交互式确认
    println!("You are about to bind this server to:");
    println!("  Project:     {}", target.project.cyan().bold());
    println!("  Environment: {}", target.environment.cyan().bold());
    println!("  Full Domain: {}.{}.ops.autos", target.environment, target.project);
    println!();
    
    if !prompt_confirm("Do you want to continue?")? {
        println!("Operation cancelled.");
        return Ok(());
    }

    // 2. 询问是否强制重置密钥 (解决 invalid format 问题)
    // 默认建议在遇到问题时选择 Yes
    let mut force_reset = false;
    println!();
    println!("{}", "Tip: If you are having trouble with 'ops ssh' (invalid key format), please choose Yes below.".dimmed());
    if prompt_confirm("Do you want to regenerate CI/CD SSH keys for this environment?")? {
        force_reset = true;
    }

    println!("\nChecking local SSH key...");
    let pubkey = ssh::get_default_pubkey()?;
    println!("{}", "✔ SSH key ready.".green());

    println!("Binding server...");
    
    // 传递 force_reset 参数
    let res = api::set_node(&token, &target.project, &target.environment, &pubkey, force_reset).await?;

    println!("{}", format!("✔ {}", res.message).green());

    println!("Adding CI public key to ~/.ssh/authorized_keys...");
    ssh::add_to_authorized_keys(&res.ci_ssh_public_key)?;
    
    if force_reset {
        println!("{}", "✔ CI keys have been regenerated. Please try `ops ssh` again.".green());
    }
    
    println!("{}", "✔ Setup complete!".green());
    Ok(())
}