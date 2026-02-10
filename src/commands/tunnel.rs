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

    // 6. Upload nginx config
    o_step!("{}", "Configuring nginx...".cyan());

    let nginx_conf = format!(
        r#"server {{
    listen 80;
    server_name {domain};

    location / {{
        proxy_pass http://127.0.0.1:{remote_port};
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 86400;
        proxy_buffering off;
    }}
}}
"#,
        domain = domain,
        remote_port = remote_port,
    );

    let conf_name = format!("ops-tunnel-{}-{}.conf", subdomain, project_name);

    // Upload via SSH stdin
    let upload_cmd = format!("cat > /etc/nginx/sites-available/{}", conf_name);
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
        stdin.write_all(nginx_conf.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        let _ = api::delete_tunnel(&token, tunnel_id).await;
        return Err(anyhow!("Failed to upload nginx config"));
    }

    // Enable and reload nginx
    let enable_cmd = format!(
        "ln -sf /etc/nginx/sites-available/{conf} /etc/nginx/sites-enabled/ && nginx -t && systemctl reload nginx",
        conf = conf_name,
    );
    let status = Command::new("ssh")
        .arg("-i").arg(&key_path)
        .arg("-o").arg("StrictHostKeyChecking=no")
        .arg("-o").arg("UserKnownHostsFile=/dev/null")
        .arg("-o").arg("LogLevel=ERROR")
        .arg(&ssh_target)
        .arg(&enable_cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        let _ = cleanup_nginx(&key_path, &ssh_target, &conf_name);
        let _ = api::delete_tunnel(&token, tunnel_id).await;
        return Err(anyhow!("Failed to enable nginx config"));
    }

    o_success!("   {} nginx reloaded", "✔".green());

    // 6.5. SSL via certbot
    o_step!("{}", "Requesting SSL certificate...".cyan());
    let certbot_cmd = format!(
        "certbot --nginx -d {} --non-interactive --agree-tos --email admin@ops.autos",
        domain
    );
    let certbot_status = Command::new("ssh")
        .arg("-i").arg(&key_path)
        .arg("-o").arg("StrictHostKeyChecking=no")
        .arg("-o").arg("UserKnownHostsFile=/dev/null")
        .arg("-o").arg("LogLevel=ERROR")
        .arg(&ssh_target)
        .arg(&certbot_cmd)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    let protocol = match certbot_status {
        Ok(s) if s.success() => {
            o_success!("   {} SSL certificate issued", "✔".green());
            "https"
        }
        _ => {
            o_warn!("   {} certbot failed, using HTTP", "⚠".yellow());
            "http"
        }
    };

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
    o_detail!("   Removing nginx config...");
    let _ = cleanup_nginx(&key_path_clone, &ssh_target_clone, &conf_name_clone);

    o_detail!("   Removing DNS record...");
    let _ = api::delete_tunnel(&token, tunnel_id).await;

    o_result!("{}", "Tunnel closed.".green());
    Ok(())
}

fn cleanup_nginx(key_path: &str, ssh_target: &str, conf_name: &str) -> Result<()> {
    let cmd = format!(
        "rm -f /etc/nginx/sites-enabled/{conf} /etc/nginx/sites-available/{conf} && nginx -t && systemctl reload nginx",
        conf = conf_name,
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
