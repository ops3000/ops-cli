use anyhow::Result;
use std::process::Command;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;

pub fn get_logs(compose_dir: &str, service: &str, lines: u32) -> Result<String> {
    let output = Command::new("docker")
        .args(["compose", "logs", "--tail", &lines.to_string(), "--no-color", service])
        .current_dir(compose_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker compose logs failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub async fn stream_logs(
    compose_dir: &str,
    service: &str,
    sender: tokio::sync::mpsc::Sender<String>,
) -> Result<()> {
    let mut child = TokioCommand::new("docker")
        .args(["compose", "logs", "-f", "--tail", "50", "--no-color", service])
        .current_dir(compose_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("No stdout"))?;
    let mut reader = BufReader::new(stdout).lines();

    while let Ok(Some(line)) = reader.next_line().await {
        if sender.send(line).await.is_err() {
            break;
        }
    }

    let _ = child.kill().await;
    Ok(())
}
