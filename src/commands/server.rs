use crate::{api, config};
use anyhow::Result;
use colored::Colorize;

pub async fn handle_server_whoami() -> Result<()> {
    let token = config::load_config().ok().and_then(|cfg| cfg.token);

    match api::server_whoami(token.as_deref()).await {
        Ok(res) => {
            o_result!("{} {}", "Server IP:".bold(), res.ip_address);
            if res.status == "bound" {
                o_detail!("{}   {}", "Status:".bold(), res.status.green());
                o_detail!("{} {}", "Project:".bold(), res.project.unwrap_or_default().cyan());
                o_detail!("{}  {}", "Domain:".bold(), res.domain.unwrap_or_default());
                o_detail!("{}   {}", "Owner:".bold(), res.owner.unwrap_or_default());
                o_detail!("{}    {}", "Permission:".bold(), res.permission.unwrap_or_default());
            } else {
                o_detail!("{} {}", "Status:".bold(), res.status.yellow());
                o_detail!("{}", res.message.unwrap_or_default());
            }
        },
        Err(e) => {
            // 对404情况的更优雅处理
            if e.to_string().contains("unbound") {
                 o_result!("{} {}", "Server IP:".bold(), "Unknown (request failed)");
                 o_detail!("{} {}", "Status:".bold(), "unbound".yellow());
                 o_detail!("This server is not currently bound to any project in ops.autos.");
            } else {
                 return Err(e);
            }
        }
    }
    Ok(())
}