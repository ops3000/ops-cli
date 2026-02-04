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
    deploy_with_repo(compose_dir, None, None)
}

pub fn deploy_with_repo(deploy_path: &str, git_repo: Option<&str>, branch: Option<&str>) -> Result<ActionResult> {
    let git_dir = std::path::Path::new(deploy_path).join(".git");
    let branch = branch.unwrap_or("main");

    // Check if .git exists
    if !git_dir.exists() {
        // Need to clone
        if let Some(repo) = git_repo {
            // Create parent directory if needed
            if let Some(parent) = std::path::Path::new(deploy_path).parent() {
                std::fs::create_dir_all(parent)?;
            }

            let clone_output = Command::new("git")
                .args(["clone", "--branch", branch, repo, deploy_path])
                .output()?;

            if !clone_output.status.success() {
                let stderr = String::from_utf8_lossy(&clone_output.stderr);
                return Ok(ActionResult {
                    success: false,
                    message: format!("git clone failed: {}", stderr),
                });
            }
        } else {
            return Ok(ActionResult {
                success: false,
                message: format!("No git repository at {} and no repo URL provided", deploy_path),
            });
        }
    } else {
        // git pull
        let git_output = Command::new("git")
            .args(["pull"])
            .current_dir(deploy_path)
            .output()?;

        if !git_output.status.success() {
            let stderr = String::from_utf8_lossy(&git_output.stderr);
            return Ok(ActionResult {
                success: false,
                message: format!("git pull failed: {}", stderr),
            });
        }
    }

    // docker compose up -d --build
    let output = Command::new("docker")
        .args(["compose", "up", "-d", "--build"])
        .current_dir(deploy_path)
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
