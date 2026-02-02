use anyhow::Result;
use serde::Serialize;
use std::process::Command;

#[derive(Serialize, Debug)]
pub struct Container {
    pub name: String,
    pub service: String,
    pub state: String,
    pub status: String,
    pub ports: String,
}

pub fn list_containers(compose_dir: &str) -> Result<Vec<Container>> {
    let output = Command::new("docker")
        .args(["compose", "ps", "--format", "json", "-a"])
        .current_dir(compose_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker compose ps failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut containers = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            containers.push(Container {
                name: v["Name"].as_str().unwrap_or("").to_string(),
                service: v["Service"].as_str().unwrap_or("").to_string(),
                state: v["State"].as_str().unwrap_or("").to_string(),
                status: v["Status"].as_str().unwrap_or("").to_string(),
                ports: v["Ports"].as_str().unwrap_or("").to_string(),
            });
        }
    }

    Ok(containers)
}

pub fn list_services(compose_dir: &str) -> Result<Vec<String>> {
    let output = Command::new("docker")
        .args(["compose", "config", "--services"])
        .current_dir(compose_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker compose config --services failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
}
