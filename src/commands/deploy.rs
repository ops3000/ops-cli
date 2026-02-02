use crate::types::OpsToml;
use crate::commands::ssh;
use crate::commands::scp;
use crate::{api, config, utils};
use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

/// 读取并解析 ops.toml
pub fn load_ops_toml(path: &str) -> Result<OpsToml> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Cannot read {}", path))?;
    let config: OpsToml = toml::from_str(&content)
        .with_context(|| format!("Invalid ops.toml format in {}", path))?;
    Ok(config)
}

/// ops deploy 主入口
pub async fn handle_deploy(
    file: String,
    service_filter: Option<String>,
    restart_only: bool,
) -> Result<()> {
    // 1. 解析配置
    println!("{}", "📦 Reading ops.toml...".cyan());
    let config = load_ops_toml(&file)?;
    println!("   App: {} → {}", config.app.green(), config.target.cyan());

    let target = &config.target;
    let deploy_path = &config.deploy_path;

    // 2. 确保远程目录存在
    println!("\n{}", "🔑 Connecting...".cyan());
    ssh::execute_remote_command(target, &format!("mkdir -p {}", deploy_path), None).await?;

    // 3. 同步代码
    if !restart_only {
        sync_code(&config).await?;
    }

    // 4. 同步 env 文件
    sync_env_files(&config).await?;

    // 5. 同步额外目录
    sync_directories(&config).await?;

    // 6. 构建 & 启动 (用户自己的 docker-compose.yml)
    build_and_start(&config, &service_filter, restart_only).await?;

    // 7. Nginx 路由 + SSL
    if !config.routes.is_empty() && !restart_only {
        generate_and_upload_nginx(&config).await?;
    }

    // 8. 健康检查
    run_health_checks(&config).await?;

    println!(
        "\n{} Deployed {} to {}",
        "✅".green(),
        config.app.green(),
        config.target.cyan()
    );
    Ok(())
}

// ===== 内部函数 =====

async fn sync_code(config: &OpsToml) -> Result<()> {
    let target = &config.target;
    let deploy_path = &config.deploy_path;

    match config.deploy.source.as_str() {
        "git" => {
            println!("\n{}", "📤 Syncing code (git)...".cyan());
            let git = config
                .deploy
                .git
                .as_ref()
                .context("deploy.source='git' requires [deploy.git] section")?;
            let branch = config.deploy.branch.as_deref().unwrap_or("main");

            // 检查远程是否已有 .git 目录
            let check = format!(
                "test -d {}/.git && echo 'exists' || echo 'missing'",
                deploy_path
            );
            let output = ssh::execute_remote_command_with_output(target, &check).await?;
            let output_str = String::from_utf8_lossy(&output).trim().to_string();

            if output_str == "exists" {
                let cmd = format!("cd {} && git pull origin {}", deploy_path, branch);
                ssh::execute_remote_command(target, &cmd, None).await?;
            } else {
                // 初次 clone — 先配置 deploy key
                if let Some(key_path) = &git.ssh_key {
                    let expanded = shellexpand::tilde(key_path).to_string();
                    setup_deploy_key(target, &expanded).await?;
                }
                let cmd = format!(
                    "GIT_SSH_COMMAND='ssh -o StrictHostKeyChecking=no' git clone -b {} {} {}",
                    branch, git.repo, deploy_path
                );
                ssh::execute_remote_command(target, &cmd, None).await?;
            }
            println!("   {}", "✔ Code synced.".green());
        }
        "push" => {
            println!("\n{}", "📤 Syncing code (rsync)...".cyan());
            rsync_push(target, deploy_path).await?;
            println!("   {}", "✔ Code synced.".green());
        }
        other => return Err(anyhow::anyhow!("Unknown deploy source: {}", other)),
    }
    Ok(())
}

/// 上传 deploy key 到服务器并配置 SSH
async fn setup_deploy_key(target: &str, local_key_path: &str) -> Result<()> {
    let key_content = fs::read_to_string(local_key_path)
        .with_context(|| format!("Cannot read deploy key: {}", local_key_path))?;

    ssh::execute_remote_command(
        target,
        "mkdir -p ~/.ssh && cat > ~/.ssh/deploy_key && chmod 600 ~/.ssh/deploy_key",
        Some(&key_content),
    )
    .await?;

    ssh::execute_remote_command(
        target,
        r#"grep -q 'deploy_key' ~/.ssh/config 2>/dev/null || cat >> ~/.ssh/config << 'SSHEOF'
Host github.com
  IdentityFile ~/.ssh/deploy_key
  StrictHostKeyChecking no
SSHEOF
chmod 600 ~/.ssh/config"#,
        None,
    )
    .await?;

    println!("   {}", "✔ Deploy key configured.".green());
    Ok(())
}

async fn rsync_push(target_str: &str, deploy_path: &str) -> Result<()> {
    let target = utils::parse_target(target_str)?;
    let full_domain = format!("{}.{}.ops.autos", target.environment, target.project);

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    let key_resp = api::get_ci_private_key(&token, &target.project, &target.environment).await?;

    let mut temp_key_file = tempfile::NamedTempFile::new()?;
    writeln!(temp_key_file, "{}", key_resp.private_key)?;
    let meta = temp_key_file.as_file().metadata()?;
    let mut perms = meta.permissions();
    perms.set_mode(0o600);
    temp_key_file.as_file().set_permissions(perms)?;
    let key_path = temp_key_file.path().to_str().unwrap().to_string();

    let ssh_cmd = format!(
        "ssh -i {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null",
        key_path
    );
    let remote = format!("root@{}:{}/", full_domain, deploy_path);

    println!("   ./ → {}", remote);

    let status = Command::new("rsync")
        .arg("-az")
        .arg("--delete")
        .arg("-e").arg(&ssh_cmd)
        .arg("--exclude").arg("target/")
        .arg("--exclude").arg("node_modules/")
        .arg("--exclude").arg(".git/")
        .arg("--exclude").arg(".env")
        .arg("--exclude").arg(".env.deploy")
        .arg("./")
        .arg(&remote)
        .status()
        .context("Failed to execute rsync (is rsync installed?)")?;

    if !status.success() {
        return Err(anyhow::anyhow!("rsync failed with status: {}", status));
    }
    Ok(())
}

async fn sync_env_files(config: &OpsToml) -> Result<()> {
    if config.env_files.is_empty() {
        return Ok(());
    }

    let target = &config.target;
    let deploy_path = &config.deploy_path;
    let mut printed_header = false;

    for ef in &config.env_files {
        if Path::new(&ef.local).exists() {
            if !printed_header {
                println!("\n{}", "📤 Syncing env files...".cyan());
                printed_header = true;
            }
            let content = fs::read_to_string(&ef.local)?;
            let remote_path = format!("{}/{}", deploy_path, ef.remote);
            ssh::execute_remote_command(
                target,
                &format!("cat > {}", remote_path),
                Some(&content),
            )
            .await?;
            println!("   ✔ {} → {}", ef.local.cyan(), remote_path);
        }
    }
    Ok(())
}

async fn sync_directories(config: &OpsToml) -> Result<()> {
    if config.sync.is_empty() {
        return Ok(());
    }

    let target = &config.target;
    let deploy_path = &config.deploy_path;
    let mut printed_header = false;

    for s in &config.sync {
        if Path::new(&s.local).exists() {
            if !printed_header {
                println!("\n{}", "📤 Syncing directories...".cyan());
                printed_header = true;
            }
            let remote = format!("{}:{}/{}", target, deploy_path, s.remote);
            println!("   {} → {}", s.local.cyan(), remote);
            scp::handle_push(s.local.clone(), remote).await?;
        }
    }
    Ok(())
}

async fn generate_and_upload_nginx(config: &OpsToml) -> Result<()> {
    println!("\n{}", "⚙️  Generating nginx config...".cyan());
    let target = &config.target;

    let mut nginx = String::new();
    for route in &config.routes {
        nginx.push_str(&format!(
            r#"server {{
    listen 80;
    server_name {domain};

    location / {{
        proxy_pass http://127.0.0.1:{port};
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 86400;
        proxy_buffering off;
        proxy_cache off;
        chunked_transfer_encoding on;
    }}
}}

"#,
            domain = route.domain,
            port = route.port
        ));

        println!(
            "   ✔ {} → :{}",
            route.domain.green(),
            route.port
        );
    }

    // 上传 per-app 配置文件
    let conf_name = format!("ops-{}.conf", config.app);
    ssh::execute_remote_command(
        target,
        &format!("cat > /etc/nginx/sites-available/{}", conf_name),
        Some(&nginx),
    )
    .await?;

    // 启用 & reload
    ssh::execute_remote_command(
        target,
        &format!("ln -sf /etc/nginx/sites-available/{conf} /etc/nginx/sites-enabled/ && nginx -t && systemctl reload nginx", conf = conf_name),
        None,
    )
    .await?;

    // SSL (certbot)
    let ssl_domains: Vec<&str> = config
        .routes
        .iter()
        .filter(|r| r.ssl)
        .map(|r| r.domain.as_str())
        .collect();

    if !ssl_domains.is_empty() {
        let domain_args = ssl_domains
            .iter()
            .map(|d| format!("-d {}", d))
            .collect::<Vec<_>>()
            .join(" ");
        let certbot_cmd = format!(
            "which certbot > /dev/null 2>&1 && certbot --nginx {} --non-interactive --agree-tos --email admin@{} || echo 'certbot not installed, skipping SSL'",
            domain_args, ssl_domains[0]
        );
        ssh::execute_remote_command(target, &certbot_cmd, None).await?;
    }

    Ok(())
}

async fn build_and_start(
    config: &OpsToml,
    filter: &Option<String>,
    restart_only: bool,
) -> Result<()> {
    let target = &config.target;
    let deploy_path = &config.deploy_path;

    println!("\n{}", "🚀 Building & starting services...".cyan());

    let svc_arg = match filter {
        Some(s) => format!(" {}", s),
        None => String::new(),
    };

    if restart_only {
        let cmd = format!("cd {} && docker compose restart{}", deploy_path, svc_arg);
        ssh::execute_remote_command(target, &cmd, None).await?;
    } else {
        let cmd = format!(
            "cd {} && docker compose build{} && docker compose up -d --remove-orphans{}",
            deploy_path, svc_arg, svc_arg
        );
        ssh::execute_remote_command(target, &cmd, None).await?;
    }

    Ok(())
}

async fn run_health_checks(config: &OpsToml) -> Result<()> {
    if config.healthchecks.is_empty() {
        return Ok(());
    }

    let target = &config.target;
    println!("\n{}", "💚 Health checks:".cyan());

    for hc in &config.healthchecks {
        let cmd = format!(
            "for i in 1 2 3 4 5 6 7 8 9 10; do curl -sf {} > /dev/null && echo 'OK' && exit 0; sleep 2; done; echo 'FAIL'; exit 1",
            hc.url
        );
        let output = ssh::execute_remote_command_with_output(target, &cmd).await;
        match output {
            Ok(o) if String::from_utf8_lossy(&o).trim() == "OK" => {
                println!("   ✔ {}  {}  {}", hc.name.green(), hc.url, "OK".green());
            }
            _ => {
                println!("   ✘ {}  {}  {}", hc.name.red(), hc.url, "FAILED".red());
            }
        }
    }
    Ok(())
}
