use crate::{api, config};
use anyhow::Result;
use colored::Colorize;

pub async fn handle_server_whoami() -> Result<()> {
    let token = config::load_config().ok().and_then(|cfg| cfg.token);

    match api::server_whoami(token.as_deref()).await {
        Ok(res) => {
            println!("{} {}", "Server IP:".bold(), res.ip_address);
            if res.status == "bound" {
                println!("{}   {}", "Status:".bold(), res.status.green());
                println!("{} {}", "Project:".bold(), res.project.unwrap_or_default().cyan());
                println!("{}  {}", "Domain:".bold(), res.domain.unwrap_or_default());
                println!("{}   {}", "Owner:".bold(), res.owner.unwrap_or_default());
                println!("{}    {}", "Permission:".bold(), res.permission.unwrap_or_default());
            } else {
                println!("{} {}", "Status:".bold(), res.status.yellow());
                println!("{}", res.message.unwrap_or_default());
            }
        },
        Err(e) => {
            // 对404情况的更优雅处理
            if e.to_string().contains("unbound") {
                 println!("{} {}", "Server IP:".bold(), "Unknown (request failed)");
                 println!("{} {}", "Status:".bold(), "unbound".yellow());
                 println!("This server is not currently bound to any project in ops.autos.");
            } else {
                 return Err(e);
            }
        }
    }
    Ok(())
}