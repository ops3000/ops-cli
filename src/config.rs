use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};
use std::env; // 引入 env

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