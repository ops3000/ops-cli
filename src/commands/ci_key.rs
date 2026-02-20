use crate::{api, config, utils};
use crate::utils::Target;
use anyhow::{Context, Result};

/// Get CI private key for a target
/// Supports both Node ID (e.g., "12345") and App target (e.g., "api.RedQ")
pub async fn handle_get_ci_private_key(target_str: String) -> Result<()> {
    let target = utils::parse_target(&target_str)?;

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login`")?;

    let private_key = match &target {
        Target::NodeId { id, .. } => {
            let res = api::get_node_ci_key(&token, *id).await?;
            res.private_key
        }
        Target::AppTarget { app, project, .. } => {
            let res = api::get_app_ci_key(&token, project, app).await?;
            res.private_key
        }
    };

    println!("{}", private_key);

    Ok(())
}