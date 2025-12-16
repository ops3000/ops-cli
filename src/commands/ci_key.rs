use crate::{api, config, utils};
use anyhow::{Context, Result};

pub async fn handle_get_ci_private_key(target_str: String) -> Result<()> {
    let target = utils::parse_target(&target_str)?;

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login`")?;

    let res = api::get_ci_private_key(&token, &target.project, &target.environment).await?;
    
    println!("{}", res.private_key);

    Ok(())
}