// src/utils.rs

use anyhow::{anyhow, Result};

pub struct Target {
    pub project: String,
    pub environment: String,
}

// 解析 "main_server.jug0" -> environment="main_server", project="jug0"
pub fn parse_target(target: &str) -> Result<Target> {
    let parts: Vec<&str> = target.split('.').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid target format. Expected 'environment.project' (e.g., main_server.jug0)"));
    }

    Ok(Target {
        environment: parts[0].to_string(),
        project: parts[1].to_string(),
    })
}