use crate::{api, config};
use anyhow::{Context, Result};
use colored::Colorize;
use std::process::Command;
use std::io::{BufRead, BufReader};
use std::fs;
use std::path::Path;

/// Get the user's SSH public key
fn get_ssh_public_key() -> Result<String> {
    let home = std::env::var("HOME").context("Could not find HOME directory")?;
    let ssh_dir = Path::new(&home).join(".ssh");

    // Try common key types in order of preference
    let key_files = ["id_ed25519.pub", "id_rsa.pub", "id_ecdsa.pub"];

    for key_file in &key_files {
        let key_path = ssh_dir.join(key_file);
        if key_path.exists() {
            let content = fs::read_to_string(&key_path)
                .context(format!("Failed to read {}", key_path.display()))?;
            return Ok(content.trim().to_string());
        }
    }

    Err(anyhow::anyhow!(
        "No SSH public key found. Please generate one with: ssh-keygen -t ed25519"
    ))
}

/// Configure and start ops serve as a systemd service
async fn configure_serve_daemon(
    token: &str,
    port: u16,
    node_id: u64,
    compose_dir: Option<&str>,
) -> Result<()> {
    let domain = format!("{}.node.ops.autos", node_id);
    let compose_directory = compose_dir.unwrap_or("/root");

    println!("Configuring ops serve daemon...");

    // Create systemd service file
    let service_content = format!(r#"[Unit]
Description=OPS Serve - Node {}
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
ExecStart=/usr/local/bin/ops serve --token {} --port {} --compose-dir {}
Restart=always
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
"#, node_id, token, port, compose_directory);

    let service_path = "/etc/systemd/system/ops-serve.service";

    // Check if running as root
    if std::env::var("USER").unwrap_or_default() != "root" {
        println!("{}", "Warning: Not running as root. Cannot install systemd service.".yellow());
        println!("To install manually, create {}:", service_path);
        println!("{}", service_content.dimmed());
        return Ok(());
    }

    fs::write(service_path, &service_content)
        .context("Failed to write systemd service file")?;

    // Reload systemd and start service
    let commands = [
        ("systemctl", vec!["daemon-reload"]),
        ("systemctl", vec!["enable", "ops-serve"]),
        ("systemctl", vec!["start", "ops-serve"]),
    ];

    for (cmd, args) in &commands {
        let status = Command::new(cmd)
            .args(args)
            .status()
            .context(format!("Failed to run {} {:?}", cmd, args))?;

        if !status.success() {
            println!("{}", format!("Warning: {} {:?} failed", cmd, args).yellow());
        }
    }

    println!("{}", "✔ ops serve daemon installed and started".green());

    // Configure nginx if available
    if Path::new("/etc/nginx").exists() {
        configure_nginx(&domain, port)?;
    }

    Ok(())
}

/// Configure nginx reverse proxy for ops serve
fn configure_nginx(domain: &str, port: u16) -> Result<()> {
    let nginx_config = format!(r#"server {{
    listen 80;
    server_name {};

    location / {{
        proxy_pass http://127.0.0.1:{};
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }}
}}
"#, domain, port);

    let config_path = format!("/etc/nginx/sites-available/{}", domain);
    let enabled_path = format!("/etc/nginx/sites-enabled/{}", domain);

    fs::write(&config_path, &nginx_config)
        .context("Failed to write nginx config")?;

    // Create symlink if not exists
    if !Path::new(&enabled_path).exists() {
        std::os::unix::fs::symlink(&config_path, &enabled_path)
            .context("Failed to enable nginx site")?;
    }

    // Test and reload nginx
    let test = Command::new("nginx")
        .arg("-t")
        .status()
        .context("Failed to test nginx config")?;

    if test.success() {
        Command::new("systemctl")
            .args(["reload", "nginx"])
            .status()
            .context("Failed to reload nginx")?;
        println!("{}", format!("✔ nginx configured for {}", domain).green());
    } else {
        println!("{}", "Warning: nginx config test failed".yellow());
    }

    Ok(())
}

/// Handle `ops init` command
/// Initializes this server as a node in the OPS platform
pub async fn handle_init(
    daemon: bool,
    projects: Option<String>,
    apps: Option<String>,
    region: Option<String>,
    port: u16,
    hostname: Option<String>,
    compose_dir: Option<String>,
) -> Result<()> {
    // 1. Check if logged in
    let cfg = config::load_config()
        .context("Could not load config. Please log in with `ops login` first.")?;
    let token = cfg.token
        .context("You are not logged in. Please run `ops login` first.")?;

    // 2. Get SSH public key
    println!("Reading SSH public key...");
    let ssh_pub_key = get_ssh_public_key()?;
    println!("{}", "✔ SSH public key found".green());

    // 3. Parse allowed projects and apps
    let allowed_projects: Option<Vec<String>> = projects
        .map(|p| p.split(',').map(|s| s.trim().to_string()).collect());
    let allowed_apps: Option<Vec<String>> = apps
        .map(|a| a.split(',').map(|s| s.trim().to_string()).collect());

    // 4. Initialize node
    println!("Initializing node...");
    if let Some(ref r) = region {
        println!("  Region: {}", r.cyan());
    }
    if let Some(ref p) = allowed_projects {
        println!("  Allowed projects: {}", p.join(", ").cyan());
    }
    if let Some(ref a) = allowed_apps {
        println!("  Allowed apps: {}", a.join(", ").cyan());
    }
    println!("  Serve port: {}", port.to_string().cyan());

    let res = api::init_node(
        &token,
        &ssh_pub_key,
        region.as_deref(),
        allowed_projects,
        allowed_apps,
        Some(port),
        hostname.as_deref(),
    ).await?;

    println!();
    println!("{}", "✔ Node initialized successfully!".green().bold());
    println!();
    println!("Node Details:");
    println!("  ID:          {}", res.node_id.to_string().cyan().bold());
    println!("  Domain:      {}", res.domain.cyan());
    println!("  IP Address:  {}", res.ip_address);
    println!("  Serve Port:  {}", res.serve_port);
    if let Some(r) = res.region {
        println!("  Region:      {}", r);
    }

    // 5. Configure serve daemon if requested
    if daemon {
        println!();
        configure_serve_daemon(
            &res.serve_token,
            res.serve_port,
            res.node_id as u64,
            compose_dir.as_deref(),
        ).await?;
    }

    // 6. Print CI key info
    println!();
    println!("{}", "CI Key (add to remote authorized_keys):".yellow());
    println!("{}", res.ci_ssh_public_key.dimmed());

    // 7. Print next steps
    println!();
    println!("{}", "Next steps:".yellow());
    println!("  1. Add the CI key above to ~/.ssh/authorized_keys on this server");
    println!("  2. Bind this node to an app:");
    println!("     ops set api.MyProject --node {}", res.node_id);
    println!("  3. Or use directly:");
    println!("     ops ssh {}", res.node_id);

    Ok(())
}
