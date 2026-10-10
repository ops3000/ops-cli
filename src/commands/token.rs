// src/commands/token.rs
use crate::{api, config, prompt};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;

pub async fn handle_get_token() -> Result<()> {
    let cfg = config::load_config().context("Could not load config. Are you logged in?")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;
    print!("{}", token); // 直接打印，不带换行，方便脚本捕获
    Ok(())
}

/// `ops token create --name <n> [--ttl-days <d>]` —— 给 CI / 另一台机器签一张有名字的令牌。
/// 令牌只打到 stdout (方便 `| gh secret set OPS_TOKEN`), 提示信息走 stderr。
pub async fn handle_create(name: &str, ttl_days: u32) -> Result<()> {
    let cfg = config::load_config().context("Could not load config. Are you logged in?")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;
    let res = api::create_token(&token, name, ttl_days).await?;
    print!("{}", res.token);
    eprintln!(
        "\n{} token '{}' valid until {} (or until `ops token revoke-all`)",
        "✔".green(),
        name,
        res.expires_at.unwrap_or_else(|| "?".into())
    );
    Ok(())
}

/// `ops token revoke-all` —— 抬本账号的令牌版本号: 所有机器、CI、旧电脑上的令牌一次作废,
/// 这台机器当场换上新令牌, 不会把自己锁在外面。
pub async fn handle_revoke_all(force: bool, interactive: bool) -> Result<()> {
    if std::env::var("OPS_TOKEN").map(|t| !t.is_empty()).unwrap_or(false) {
        return Err(anyhow!(
            "OPS_TOKEN is set: revoke-all would kill it and the replacement would have nowhere to go. \
             Run this on a machine with a saved login (`ops login`)."
        ));
    }
    let cfg = config::load_config().context("Could not load config. Are you logged in?")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    o_warn!("{}", "This revokes EVERY token of your account: all other machines, CI secrets, old laptops.".yellow());
    o_detail!("This machine gets a fresh token and stays logged in. Everything else must log in again");
    o_detail!("or be given a new one with `ops token create`.");
    if !force {
        if !interactive {
            return Err(anyhow!("Destructive operation requires --force in non-interactive mode"));
        }
        if !prompt::confirm_no("Revoke all tokens?", interactive)? {
            o_warn!("Aborted.");
            return Ok(());
        }
    }

    let res = api::revoke_all_tokens(&token).await?;
    config::save_config(&config::Config { token: Some(res.token) })
        .context("Tokens were revoked but the fresh one could not be saved — run `ops login`")?;
    o_success!("✔ {}", res.message);
    o_detail!("  Token version is now {}. This machine's new login is valid until {}.",
        res.token_version, res.expires_at.unwrap_or_else(|| "?".into()));
    Ok(())
}
