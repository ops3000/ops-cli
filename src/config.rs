use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use anyhow::{Context, Result};
use std::env; // 引入 env

// Active org slug for the current CLI invocation. Set once when ops.toml is
// loaded; overridden by OPS_ORG env var. None = let the backend default to
// the user's personal org.
static ACTIVE_ORG: RwLock<Option<String>> = RwLock::new(None);

pub fn set_active_org(slug: Option<String>) {
    let cleaned = slug.and_then(|s| {
        let t = s.trim();
        if t.is_empty() { None } else { Some(t.to_string()) }
    });
    if let Ok(mut w) = ACTIVE_ORG.write() {
        *w = cleaned;
    }
}

pub fn current_org() -> Option<String> {
    if let Ok(v) = env::var("OPS_ORG") {
        let t = v.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    ACTIVE_ORG.read().ok().and_then(|g| g.clone())
}

const CONFIG_DIR: &str = "ops";
const CONFIG_FILE: &str = "credentials.json";

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Config {
    pub token: Option<String>,
}

fn get_config_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .context("Could not find config directory")?
        .join(CONFIG_DIR);
    
    fs::create_dir_all(&config_dir)?;
    
    Ok(config_dir.join(CONFIG_FILE))
}

pub fn save_config(config: &Config) -> Result<()> {
    let path = get_config_path()?;
    let content = serde_json::to_string_pretty(config)?;
    fs::write(path, content).context("Failed to write config file")
}

pub fn load_config() -> Result<Config> {
    // 1. 优先检查环境变量
    if let Ok(token) = env::var("OPS_TOKEN") {
        if !token.is_empty() {
            return Ok(Config { token: Some(token) });
        }
    }

    // 2. 其次读取文件
    let path = get_config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    
    let content = fs::read_to_string(path).context("Failed to read config file")?;
    serde_json::from_str(&content).context("Failed to parse config file")
}