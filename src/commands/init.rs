use crate::{api, config, ssh};
use anyhow::{Context, Result};
use colored::Colorize;
use serde::Deserialize;
use std::io::{self, Write};
use std::process::Command;
use std::fs;
use std::path::Path;

/// Get the user's SSH public key
fn get_ssh_public_key() -> Result<String> {
    let home = std::env::var("HOME").context("Could not find HOME directory")?;
    let ssh_dir = Path::new(&home).join(".ssh");

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

/// Check and clean up old version residue files
fn cleanup_old_residue() -> Result<bool> {
    let mut found_residue = false;
    let mut cleaned = Vec::new();

    // 1. Check systemd service file
    let service_path = Path::new("/etc/systemd/system/ops-serve.service");
    if service_path.exists() {
        found_residue = true;
        // Stop and disable service first
        let _ = Command::new("systemctl").args(["stop", "ops-serve"]).status();
        let _ = Command::new("systemctl").args(["disable", "ops-serve"]).status();
        if fs::remove_file(service_path).is_ok() {
            cleaned.push(service_path.to_string_lossy().to_string());
        }
        let _ = Command::new("systemctl").args(["daemon-reload"]).status();
    }

    // 2. Check nginx configs for *.node.ops.autos
    let nginx_available = Path::new("/etc/nginx/sites-available");
    let nginx_enabled = Path::new("/etc/nginx/sites-enabled");

    if nginx_available.exists() {
        if let Ok(entries) = fs::read_dir(nginx_available) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".node.ops.autos") {
                    found_residue = true;
                    let available_path = nginx_available.join(&name);
                    let enabled_path = nginx_enabled.join(&name);

                    if fs::remove_file(&enabled_path).is_ok() {
                        cleaned.push(enabled_path.to_string_lossy().to_string());
                    }
                    if fs::remove_file(&available_path).is_ok() {
                        cleaned.push(available_path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    // 3. Check for old SSL certs
    let cert_paths = [
        "/etc/ssl/certs/ops-serve.crt",
        "/etc/ssl/private/ops-serve.key",
    ];
    for cert_path in &cert_paths {
        let path = Path::new(cert_path);
        if path.exists() {
            found_residue = true;
            if fs::remove_file(path).is_ok() {
                cleaned.push(cert_path.to_string());
            }
        }
    }

    if found_residue {
        println!("{}", "Found old OPS configuration, cleaning up...".yellow());
        for path in &cleaned {
            println!("  Removed: {}", path.dimmed());
        }
        if !cleaned.is_empty() {
            println!("{}", "✔ Old configuration cleaned".green());
        }
        // Reload nginx if we modified its config
        if cleaned.iter().any(|p| p.contains("nginx")) {
            let _ = Command::new("systemctl").args(["reload", "nginx"]).status();
        }
    }

    Ok(found_residue)
}

/// Prompt user for yes/no confirmation
fn confirm(prompt: &str) -> bool {
    print!("{} [y/N]: ", prompt);
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Configure and start ops serve as a systemd service
fn configure_serve_daemon(
    token: &str,
    port: u16,
    node_id: u64,
    compose_dir: &str,
) -> Result<()> {
    let domain = format!("{}.node.ops.autos", node_id);

    println!("Configuring systemd service...");

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
"#, node_id, token, port, compose_dir);

    let service_path = "/etc/systemd/system/ops-serve.service";

    // Check if running as root
    if std::env::var("USER").unwrap_or_default() != "root" {
        println!("{}", "Warning: Not running as root. Cannot install systemd service.".yellow());
        println!("Run with sudo or as root to enable auto-start.");
        return Ok(());
    }

    fs::write(service_path, &service_content)
        .context("Failed to write systemd service file")?;

    // Reload systemd and start service
    let commands = [
        ("systemctl", vec!["daemon-reload"]),
        ("systemctl", vec!["enable", "ops-serve"]),
        ("systemctl", vec!["restart", "ops-serve"]),
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

    println!("{}", "✔ ops-serve daemon installed and started".green());

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

    if !Path::new(&enabled_path).exists() {
        std::os::unix::fs::symlink(&config_path, &enabled_path)
            .context("Failed to enable nginx site")?;
    }

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

#[derive(Deserialize)]
struct GeoResponse {
    #[serde(default)]
    city: String,
    #[serde(default)]
    timezone: String,
}

/// Detect region from IP geolocation via ip-api.com
async fn detect_region() -> Option<(String, String)> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;

    let resp = client
        .get("http://ip-api.com/json")
        .send()
        .await
        .ok()?
        .json::<GeoResponse>()
        .await
        .ok()?;

    let ops_region = timezone_to_region(&resp.timezone)?;
    let label = if resp.city.is_empty() {
        resp.timezone.clone()
    } else {
        resp.city
    };

    Some((ops_region, label))
}

/// Map timezone string to OPS region
fn timezone_to_region(tz: &str) -> Option<String> {
    let region = if tz.starts_with("America/") {
        let city = &tz["America/".len()..];
        match city {
            "New_York" | "Toronto" | "Montreal" | "Detroit" | "Atlanta"
            | "Miami" | "Boston" | "Philadelphia" => "us-east",
            "Chicago" | "Denver" | "Dallas" | "Houston" | "Winnipeg"
            | "Mexico_City" => "us-central",
            "Los_Angeles" | "Vancouver" | "Seattle" | "Phoenix"
            | "San_Francisco" => "us-west",
            "Sao_Paulo" | "Buenos_Aires" | "Santiago" | "Bogota"
            | "Lima" => "sa-east",
            _ => "us-east",
        }
    } else if tz.starts_with("Europe/") {
        let city = &tz["Europe/".len()..];
        match city {
            "London" | "Dublin" | "Lisbon" => "eu-west",
            _ => "eu-central",
        }
    } else if tz.starts_with("Asia/") {
        let city = &tz["Asia/".len()..];
        match city {
            "Tokyo" | "Seoul" => "ap-northeast",
            "Shanghai" | "Hong_Kong" | "Taipei" | "Chongqing" => "ap-east",
            "Singapore" | "Jakarta" | "Bangkok" | "Ho_Chi_Minh"
            | "Kuala_Lumpur" | "Manila" => "ap-southeast",
            "Mumbai" | "Kolkata" | "Colombo" | "Karachi" => "ap-south",
            "Dubai" | "Riyadh" | "Baghdad" | "Tehran" => "me-south",
            _ => "ap-southeast",
        }
    } else if tz.starts_with("Australia/") || tz.starts_with("Pacific/Auckland") {
        "ap-southeast"
    } else if tz.starts_with("Africa/") {
        "af-south"
    } else {
        return None;
    };

    Some(region.to_string())
}

/// Prompt user to confirm or override the detected region
fn confirm_region(detected: Option<(String, String)>) -> Option<String> {
    match detected {
        Some((region, city)) => {
            println!("  Detected: {} ({})", region.cyan(), city);
            print!("  Use this region? [Y/n/custom]: ");
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let input = input.trim();

            if input.is_empty() || input.eq_ignore_ascii_case("y") || input.eq_ignore_ascii_case("yes") {
                Some(region)
            } else if input.eq_ignore_ascii_case("n") || input.eq_ignore_ascii_case("no") {
                None
            } else {
                // User typed a custom region
                Some(input.to_string())
            }
        }
        None => {
            println!("  {}", "Could not detect region automatically.".yellow());
            print!("  Enter region (or press Enter to skip): ");
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let input = input.trim();

            if input.is_empty() {
                None
            } else {
                Some(input.to_string())
            }
        }
    }
}

/// Handle `ops init` command
/// Initializes this server as a node in the OPS platform
pub async fn handle_init(
    _daemon: bool,
    _projects: Option<String>,
    _apps: Option<String>,
    region: Option<String>,
    port: u16,
    hostname: Option<String>,
    compose_dir: Option<String>,
) -> Result<()> {
    println!();
    println!("{}", "OPS Node Initialization".cyan().bold());
    println!("{}", "═══════════════════════".cyan());
    println!();

    // 1. Check if logged in
    let cfg = config::load_config()
        .context("Not logged in. Run `ops login` first.")?;
    let token = cfg.token
        .context("Not logged in. Run `ops login` first.")?;
    println!("{}", "✔ Logged in".green());

    // 2. Check and clean up old residue
    cleanup_old_residue()?;

    // 3. Get SSH public key
    let ssh_pub_key = get_ssh_public_key()?;
    println!("{}", "✔ SSH public key found".green());

    // 4. Auto-detect region if not provided
    let region = if region.is_some() {
        region
    } else {
        println!();
        println!("{}", "Detecting region...".cyan());
        let detected = detect_region().await;
        let confirmed = confirm_region(detected);
        if let Some(ref r) = confirmed {
            println!("{}", format!("✔ Region: {}", r).green());
        }
        confirmed
    };

    // 5. Try to initialize node
    println!("Registering node...");

    let res = match api::init_node(
        &token,
        &ssh_pub_key,
        region.as_deref(),
        None,
        None,
        Some(port),
        hostname.as_deref(),
    ).await {
        Ok(r) => r,
        Err(e) => {
            let err_msg = e.to_string();
            // If IP already registered, ask user if they want to overwrite
            if err_msg.contains("already registered") {
                // Extract existing node ID from error message if available
                println!();
                println!("{}", "This server is already registered as a node.".yellow());

                if confirm("Overwrite existing configuration?") {
                    api::reinit_node(
                        &token,
                        &ssh_pub_key,
                        region.as_deref(),
                        None,
                        None,
                        Some(port),
                        hostname.as_deref(),
                    ).await?
                } else {
                    println!("Aborted.");
                    return Ok(());
                }
            } else {
                return Err(e);
            }
        }
    };

    println!();
    println!("{}", "✔ Node registered".green().bold());
    println!();
    println!("  Node ID:  {}", res.node_id.to_string().cyan().bold());
    println!("  Domain:   {}", res.domain.cyan());
    println!("  IP:       {}", res.ip_address);
    match &res.region {
        Some(r) => println!("  Region:   {}", r.cyan()),
        None => println!("  Region:   {}", "(not set, use --region to configure)".dimmed()),
    }

    // 6. Add CI public key to authorized_keys
    println!();
    println!("Configuring SSH access...");
    ssh::add_to_authorized_keys(&res.ci_ssh_public_key)?;
    println!("{}", "✔ CI key added to authorized_keys".green());

    // 7. Configure systemd daemon (always)
    println!();
    let compose_directory = compose_dir.as_deref().unwrap_or("/root");
    configure_serve_daemon(
        &res.serve_token,
        res.serve_port,
        res.node_id as u64,
        compose_directory,
    )?;

    // Done
    println!();
    println!("{}", "═══════════════════════════════════════════".green());
    println!("{}", "  Node initialization complete!".green().bold());
    println!("{}", "═══════════════════════════════════════════".green());
    println!();
    println!("Access this server remotely:");
    println!("  {}", format!("ops ssh {}", res.node_id).cyan());
    println!();
    println!("Bind to an app:");
    println!("  {}", format!("ops set api.MyProject --node {}", res.node_id).cyan());

    Ok(())
}
