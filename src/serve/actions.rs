use anyhow::Result;
use serde::Serialize;
use std::process::Command;

#[derive(Serialize)]
pub struct ActionResult {
    pub success: bool,
    pub message: String,
}

pub fn restart_service(compose_dir: &str, service: &str) -> Result<ActionResult> {
    run_compose_command(compose_dir, &["restart", service], "restart")
}

pub fn stop_service(compose_dir: &str, service: &str) -> Result<ActionResult> {
    run_compose_command(compose_dir, &["stop", service], "stop")
}

pub fn start_service(compose_dir: &str, service: &str) -> Result<ActionResult> {
    run_compose_command(compose_dir, &["start", service], "start")
}

pub fn deploy(compose_dir: &str) -> Result<ActionResult> {
    // git pull
    let git_output = Command::new("git")
        .args(["pull"])
        .current_dir(compose_dir)
        .output()?;

    if !git_output.status.success() {
        let stderr = String::from_utf8_lossy(&git_output.stderr);
        return Ok(ActionResult {
            success: false,
            message: format!("git pull failed: {}", stderr),
        });
    }

    // docker compose up -d --build
    let output = Command::new("docker")
        .args(["compose", "up", "-d", "--build"])
        .current_dir(compose_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Ok(ActionResult {
            success: false,
            message: format!("docker compose up failed: {}", stderr),
        });
    }

    Ok(ActionResult {
        success: true,
        message: "Deploy completed successfully".to_string(),
    })
}

fn run_compose_command(compose_dir: &str, args: &[&str], action: &str) -> Result<ActionResult> {
    let mut cmd_args = vec!["compose"];
    cmd_args.extend_from_slice(args);

    let output = Command::new("docker")
        .args(&cmd_args)
        .current_dir(compose_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Ok(ActionResult {
            success: false,
            message: format!("{} failed: {}", action, stderr),
        });
    }

    Ok(ActionResult {
        success: true,
        message: format!("{} completed", action),
    })
}
