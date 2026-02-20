use crate::{api, config};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use rand::Rng;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

pub async fn handle_tunnel(target: String, local_port: u16, node_id: u64) -> Result<()> {
    // 1. Parse target: "webhook.redq" -> subdomain + project
    let parts: Vec<&str> = target.split('.').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid target format. Expected 'subdomain.project' (e.g., webhook.redq)"));
    }
    let subdomain = parts[0];
    let project_name = parts[1];

    // 2. Load config + token
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    // 3. Generate random remote port (10000-60000)
    let remote_port: u16 = rand::thread_rng().gen_range(10000..=60000);

    // 4. Register tunnel in backend (creates DNS + DB record)
    o_step!("{}", "Registering tunnel...".cyan());
    let tunnel_resp = api::create_tunnel(
        &token, subdomain, project_name, node_id, remote_port,
    ).await.context("Failed to register tunnel")?;

    let tunnel_id = tunnel_resp.tunnel_id;
    let domain = &tunnel_resp.domain;

    o_success!("   {} DNS: {} → {}", "✔".green(), domain.cyan(), tunnel_resp.node_ip);

    // 5. Fetch CI key for the node
    o_step!("{}", format!("Connecting to node {}...", node_id).cyan());
    let key_resp = match api::get_node_ci_key(&token, node_id).await {
        Ok(r) => r,
        Err(e) => {
            let _ = api::delete_tunnel(&token, tunnel_id).await;
            return Err(e.context("Failed to fetch CI key for node"));
        }
    };

    let mut temp_key_file = tempfile::NamedTempFile::new()?;
    writeln!(temp_key_file, "{}", key_resp.private_key)?;
    let meta = temp_key_file.as_file().metadata()?;
    let mut perms = meta.permissions();
    perms.set_mode(0o600);
    temp_key_file.as_file().set_permissions(perms)?;
    let key_path = temp_key_file.path().to_str().unwrap().to_string();

    let node_domain = format!("{}.node.ops.autos", node_id);
    let ssh_target = format!("root@{}", node_domain);

    o_success!("   {} SSH connected", "✔".green());

    // 6. Upload Caddy route fragment
    o_step!("{}", "Configuring Caddy route...".cyan());

    let target_header = format!("{}.{}", subdomain, project_name);
    let matcher_name = format!("ops_tunnel_{}_{}", subdomain, project_name).replace('-', "_");
    let caddy_snippet = format!(
        "# tunnel: {target}\n@{matcher} header X-OPS-Target {target}\nhandle @{matcher} {{\n    reverse_proxy 127.0.0.1:{port}\n}}\n",
        target = target_header,
        matcher = matcher_name,
        port = remote_port,
    );

    let conf_name = format!("ops-tunnel-{}-{}.caddy", subdomain, project_name);

    // Upload via SSH stdin
    let upload_cmd = format!("mkdir -p /etc/caddy/routes.d && cat > /etc/caddy/routes.d/{}", conf_name);
    let mut child = Command::new("ssh")
        .arg("-i").arg(&key_path)
        .arg("-o").arg("StrictHostKeyChecking=no")
        .arg("-o").arg("UserKnownHostsFile=/dev/null")
        .arg("-o").arg("LogLevel=ERROR")
        .arg(&ssh_target)
        .arg(&upload_cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(caddy_snippet.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        let _ = api::delete_tunnel(&token, tunnel_id).await;
        return Err(anyhow!("Failed to upload Caddy route"));
    }

    // Validate and reload Caddy
    let reload_cmd = "caddy validate --config /etc/caddy/Caddyfile && systemctl reload caddy";
    let status = Command::new("ssh")
        .arg("-i").arg(&key_path)
        .arg("-o").arg("StrictHostKeyChecking=no")
        .arg("-o").arg("UserKnownHostsFile=/dev/null")
        .arg("-o").arg("LogLevel=ERROR")
        .arg(&ssh_target)
        .arg(reload_cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        let _ = cleanup_caddy(&key_path, &ssh_target, &conf_name);
        let _ = api::delete_tunnel(&token, tunnel_id).await;
        return Err(anyhow!("Failed to reload Caddy config"));
    }

    o_success!("   {} Caddy reloaded", "✔".green());

    // SSL is handled by Cloudflare — always use https
    let protocol = "https";

    // 7. Open SSH reverse tunnel
    o_result!("\n   {} {}", "Tunnel URL:".green().bold(), format!("{}://{}", protocol, domain).cyan().bold());
    o_result!("   {} localhost:{}\n", "Forwarding →".green(), local_port);
    o_detail!("   Press {} to stop the tunnel\n", "Ctrl+C".yellow().bold());

    let ssh_child = Command::new("ssh")
        .arg("-i").arg(&key_path)
        .arg("-o").arg("StrictHostKeyChecking=no")
        .arg("-o").arg("UserKnownHostsFile=/dev/null")
        .arg("-o").arg("LogLevel=ERROR")
        .arg("-o").arg("ServerAliveInterval=15")
        .arg("-o").arg("ServerAliveCountMax=3")
        .arg("-N")
        .arg("-R").arg(format!("{}:127.0.0.1:{}", remote_port, local_port))
        .arg(&ssh_target)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to start SSH reverse tunnel")?;

    let ssh_child = Arc::new(Mutex::new(ssh_child));

    // 8. Wait for Ctrl+C or SSH exit
    let conf_name_clone = conf_name.clone();
    let key_path_clone = key_path.clone();
    let ssh_target_clone = ssh_target.clone();

    let child_for_wait = Arc::clone(&ssh_child);
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            o_step!("\n{}", "Shutting down tunnel...".yellow());
            // Kill the SSH process
            let mut child = ssh_child.lock().unwrap();
            let _ = child.kill();
            let _ = child.wait();
        }
        result = tokio::task::spawn_blocking(move || {
            // Poll the child process in a blocking thread
            let mut child = child_for_wait.lock().unwrap();
            child.wait()
        }) => {
            match result {
                Ok(Ok(status)) if status.success() => {
                    o_step!("\n{}", "SSH tunnel closed.".yellow());
                }
                _ => {
                    o_warn!("\n{}", "SSH tunnel exited unexpectedly.".yellow());
                }
            }
        }
    }

    // 9. Cleanup
    o_detail!("   Removing Caddy route...");
    let _ = cleanup_caddy(&key_path_clone, &ssh_target_clone, &conf_name_clone);

    o_detail!("   Removing DNS record...");
    let _ = api::delete_tunnel(&token, tunnel_id).await;

    o_result!("{}", "Tunnel closed.".green());
    Ok(())
}

fn cleanup_caddy(key_path: &str, ssh_target: &str, conf_name: &str) -> Result<()> {
    let cmd = format!(
        "rm -f /etc/caddy/routes.d/{} && caddy validate --config /etc/caddy/Caddyfile && systemctl reload caddy",
        conf_name,
    );
    Command::new("ssh")
        .arg("-i").arg(key_path)
        .arg("-o").arg("StrictHostKeyChecking=no")
        .arg("-o").arg("UserKnownHostsFile=/dev/null")
        .arg("-o").arg("LogLevel=ERROR")
        .arg(ssh_target)
        .arg(&cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(())
}
