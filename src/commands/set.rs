use crate::{api, config, ssh, utils}; // 引入 utils
use anyhow::{Context, Result};
use colored::Colorize;

// 参数改为单个 String
pub async fn handle_set(target_str: String) -> Result<()> {
    // 解析 target
    let target = utils::parse_target(&target_str)?;
    
    let cfg = config::load_config().context("Could not load config. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;
    
    println!("Checking SSH key...");
    // 这里的 get_default_pubkey 已经集成了自动创建逻辑
    let pubkey = ssh::get_default_pubkey()?;
    println!("{}", "✔ SSH key ready.".green());

    println!("Binding this server to project '{}' environment '{}'...", target.project.cyan(), target.environment.cyan());
    
    // 使用解析出来的字段
    let res = api::set_node(&token, &target.project, &target.environment, &pubkey).await?;

    println!("{}", format!("✔ {}", res.message).green());

    println!("Adding CI public key to ~/.ssh/authorized_keys...");
    ssh::add_to_authorized_keys(&res.ci_ssh_public_key)?;
    
    println!("{}", "✔ Setup complete!".green());
    Ok(())
}