use crate::{api, config};
use anyhow::{Context, Result};

pub async fn handle_get_ci_private_key(project: String, environment: String) -> Result<()> {
    let cfg = config::load_config().context("Could not load config. Have you logged in?")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    let res = api::get_ci_private_key(&token, &project, &environment).await?;
    
    println!("{}", res.private_key);

    Ok(())
}