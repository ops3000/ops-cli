// src/utils.rs

use anyhow::{anyhow, Result};

pub struct Target {
    pub project: String,
    pub environment: String,
    pub path: Option<String>, // 新增路径字段
}

// 解析 "main_server.jug0" 或 "main_server.jug0:/var/www"
pub fn parse_target(target_str: &str) -> Result<Target> {
    // 1. 分离远程路径 (如果有冒号)
    let (server_part, path_part) = match target_str.split_once(':') {
        Some((s, p)) => (s, Some(p.to_string())),
        None => (target_str, None),
    };

    // 2. 解析 environment.project
    let parts: Vec<&str> = server_part.split('.').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid target format. Expected 'environment.project' (e.g., main_server.jug0)"));
    }

    Ok(Target {
        environment: parts[0].to_string(),
        project: parts[1].to_string(),
        path: path_part,
    })
}