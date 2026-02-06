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

// ===== 辅助函数 =====

/// 解析 "$ENV_VAR" → 读环境变量值
fn resolve_env_value(val: &str) -> Result<String> {
    if val.starts_with('$') {
        std::env::var(&val[1..])
            .with_context(|| format!("Environment variable {} not set", val))
    } else {
        Ok(val.to_string())
    }
}

/// 构建 -f 参数: "-f a.yml -f b.yml"，无配置时返回空串
fn compose_file_args(config: &OpsToml) -> String {
    config.deploy.compose_files.as_ref()
        .map(|files| files.iter().map(|f| format!("-f {}", f)).collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

/// 构建环境变量前缀: "K=V K2=V2 "
fn env_prefix(env_vars: &[String]) -> String {
    if env_vars.is_empty() { return String::new(); }
    let mut s = env_vars.join(" ");
    s.push(' ');
    s
}

/// 解析 --app 到具体的 docker-compose service names
fn resolve_services(config: &OpsToml, app: &Option<String>, service: &Option<String>) -> String {
    if let Some(svc) = service {
        return svc.clone();
    }
    if let Some(app_name) = app {
        if let Some(app_def) = config.apps.iter().find(|a| a.name == *app_name) {
            return app_def.services.join(" ");
        }
    }
    String::new()  // 空 = 所有 services
}

/// 解析 app 名称：优先 app 字段（旧模式），否则 project 字段
fn resolve_app_name(config: &OpsToml) -> Result<String> {
    config.app.clone()
        .or(config.project.clone())
        .context("ops.toml must have 'app' or 'project'")
}

/// 解析部署目标：优先用 ops.toml 的 target，否则从 API 查询
async fn resolve_target(config: &OpsToml, app_filter: &Option<String>) -> Result<String> {
    // 1. 如果 ops.toml 有 target，直接用
    if let Some(ref t) = config.target {
        return Ok(t.clone());
    }

    // 2. project 模式：从 API 解析
    let project = config.project.as_ref()
        .context("ops.toml must have 'target' or 'project'")?;

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    if let Some(app_name) = app_filter {
        // --app 指定了 app，查找该 app 的主节点
        let node = api::get_app_primary_node(&token, project, app_name).await
            .with_context(|| format!("Failed to find primary node for app '{}' in project '{}'", app_name, project))?;
        Ok(node.domain)
    } else {
        // 全量部署，查找项目下的第一个节点
        let nodes_resp = api::list_nodes_v2(&token).await?;
        let node = nodes_resp.nodes.iter()
            .find(|n| n.bound_apps.as_ref().map_or(false, |apps|
                apps.iter().any(|a| a.project_name == *project)))
            .context(format!("No nodes bound to project '{}'", project))?;
        Ok(node.domain.clone())
    }
}

/// ops deploy 主入口
pub async fn handle_deploy(
    file: String,
    service_filter: Option<String>,
    app_filter: Option<String>,
    restart_only: bool,
    env_vars: Vec<String>,
) -> Result<()> {
    // 1. 解析配置
    println!("{}", "📦 Reading ops.toml...".cyan());
    let config = load_ops_toml(&file)?;

    let app_name = resolve_app_name(&config)?;
    let target = resolve_target(&config, &app_filter).await?;

    println!("   App: {} → {}", app_name.green(), target.cyan());
    if let Some(ref app) = app_filter {
        let svcs = resolve_services(&config, &app_filter, &service_filter);
        if !svcs.is_empty() {
            println!("   Group: {} → [{}]", app.yellow(), svcs);
        }
    }
    if let Some(ref svc) = service_filter {
        println!("   Service: {}", svc.yellow());
    }

    let deploy_path = &config.deploy_path;

    // 2. 同步 App 记录到后端 (可选，失败不阻塞部署)
    let (app_id, deployment_id) = sync_app_record(&config, &target).await;

    // 3. 确保远程目录存在
    println!("\n{}", "🔑 Connecting...".cyan());
    ssh::execute_remote_command(&target, &format!("mkdir -p {}", deploy_path), None).await?;

    // 4. 执行部署
    let deploy_result = execute_deployment(
        &config, &target, &service_filter, &app_filter, restart_only, &env_vars,
    ).await;

    // 5. 更新部署状态
    if let (Some(_app_id), Some(deployment_id)) = (app_id, deployment_id) {
        update_deployment_status(deployment_id, &deploy_result).await;
    }

    // 6. 返回结果
    deploy_result?;

    println!(
        "\n{} Deployed {} to {}",
        "✅".green(),
        app_name.green(),
        target.cyan()
    );
    Ok(())
}

/// 同步 App 记录到后端，返回 (app_id, deployment_id)
async fn sync_app_record(config: &OpsToml, _target: &str) -> (Option<i64>, Option<i64>) {
    // 尝试加载 token
    let cfg = match config::load_config() {
        Ok(c) => c,
        Err(_) => {
            println!("   {} (not logged in, skipping)", "⚠ App record sync skipped".yellow());
            return (None, None);
        }
    };

    let token = match cfg.token {
        Some(t) => t,
        None => {
            println!("   {} (not logged in, skipping)", "⚠ App record sync skipped".yellow());
            return (None, None);
        }
    };

    // 同步 App
    println!("{}", "📝 Syncing app record...".cyan());
    let sync_result = match api::sync_app(&token, config).await {
        Ok(r) => r,
        Err(e) => {
            println!("   {} {} (continuing anyway)", "⚠ Sync failed:".yellow(), e);
            return (None, None);
        }
    };

    let action = if sync_result.created { "Created" } else { "Updated" };
    println!("   ✔ {} app (ID: {})", action.green(), sync_result.app_id);

    // 创建部署记录
    let deployment = match api::create_deployment(&token, sync_result.app_id, "cli").await {
        Ok(d) => d,
        Err(e) => {
            println!("   {} {} (continuing anyway)", "⚠ Deployment record failed:".yellow(), e);
            return (Some(sync_result.app_id), None);
        }
    };

    println!("   ✔ Deployment #{} started", deployment.id);

    (Some(sync_result.app_id), Some(deployment.id))
}

/// 更新部署状态
async fn update_deployment_status(deployment_id: i64, result: &Result<()>) {
    let cfg = config::load_config().ok();
    let token = cfg.and_then(|c| c.token);

    if let Some(token) = token {
        let (status, logs) = match result {
            Ok(_) => ("success", None),
            Err(e) => ("failed", Some(e.to_string())),
        };

        if let Err(e) = api::update_deployment(&token, deployment_id, status, logs.as_deref()).await {
            println!("   {} {}", "⚠ Failed to update deployment status:".yellow(), e);
        }
    }
}

/// 执行实际部署流程
async fn execute_deployment(
    config: &OpsToml,
    target: &str,
    service_filter: &Option<String>,
    app_filter: &Option<String>,
    restart_only: bool,
    env_vars: &[String],
) -> Result<()> {
    // 同步代码
    if !restart_only {
        sync_code(config, target, app_filter, service_filter, env_vars).await?;
    }

    // 同步 env 文件
    sync_env_files(config, target).await?;

    // 同步额外目录
    sync_directories(config, target).await?;

    // 构建 & 启动
    build_and_start(config, target, service_filter, app_filter, restart_only, env_vars).await?;

    // Nginx 路由 + SSL
    if !config.routes.is_empty() && !restart_only {
        generate_and_upload_nginx(config, target).await?;
    }

    // 健康检查
    run_health_checks(config, target).await?;

    Ok(())
}

// ===== 内部函数 =====

async fn sync_code(
    config: &OpsToml,
    target: &str,
    app_filter: &Option<String>,
    service_filter: &Option<String>,
    env_vars: &[String],
) -> Result<()> {
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
        "image" => {
            println!("\n{}", "🐳 Pulling images...".cyan());

            // 1. Docker login
            if let Some(reg) = &config.deploy.registry {
                let user = resolve_env_value(&reg.username)?;
                let token = resolve_env_value(&reg.token)?;
                ssh::execute_remote_command(
                    target,
                    &format!("echo '{}' | docker login {} -u {} --password-stdin", token, reg.url, user),
                    None,
                ).await?;
                println!("   {}", "✔ Registry login".green());
            }

            // 2. Pull
            let compose = compose_file_args(config);
            let env = env_prefix(env_vars);
            let svcs = resolve_services(config, app_filter, service_filter);
            let cmd = format!("cd {} && {}docker compose {} pull {}", deploy_path, env, compose, svcs);
            ssh::execute_remote_command(target, &cmd, None).await?;
            println!("   {}", "✔ Images pulled".green());
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

async fn sync_env_files(config: &OpsToml, target: &str) -> Result<()> {
    if config.env_files.is_empty() {
        return Ok(());
    }

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

async fn sync_directories(config: &OpsToml, target: &str) -> Result<()> {
    if config.sync.is_empty() {
        return Ok(());
    }

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

async fn generate_and_upload_nginx(config: &OpsToml, target: &str) -> Result<()> {
    println!("\n{}", "⚙️  Generating nginx config...".cyan());

    let app_name = resolve_app_name(config)?;

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
    let conf_name = format!("ops-{}.conf", app_name);
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
    target: &str,
    service_filter: &Option<String>,
    app_filter: &Option<String>,
    restart_only: bool,
    env_vars: &[String],
) -> Result<()> {
    let deploy_path = &config.deploy_path;

    println!("\n{}", "🚀 Building & starting services...".cyan());

    let compose = compose_file_args(config);
    let env = env_prefix(env_vars);
    let svcs = resolve_services(config, app_filter, service_filter);

    // Add space before compose args and services if non-empty
    let compose_arg = if compose.is_empty() { String::new() } else { format!(" {}", compose) };
    let svc_arg = if svcs.is_empty() { String::new() } else { format!(" {}", svcs) };

    if restart_only {
        let cmd = format!("cd {} && {}docker compose{} restart{}", deploy_path, env, compose_arg, svc_arg);
        ssh::execute_remote_command(target, &cmd, None).await?;
    } else if config.deploy.source == "image" {
        // image 模式: 只 up，不 build
        let cmd = format!(
            "cd {} && {}docker compose{} up -d --remove-orphans{}",
            deploy_path, env, compose_arg, svc_arg
        );
        ssh::execute_remote_command(target, &cmd, None).await?;
        // 清理旧镜像
        ssh::execute_remote_command(target, "docker image prune -f", None).await.ok();
    } else {
        // 旧行为: build + up
        let cmd = format!(
            "cd {} && {}docker compose{} build{} && {}docker compose{} up -d --remove-orphans{}",
            deploy_path, env, compose_arg, svc_arg, env, compose_arg, svc_arg
        );
        ssh::execute_remote_command(target, &cmd, None).await?;
    }

    Ok(())
}

async fn run_health_checks(config: &OpsToml, target: &str) -> Result<()> {
    if config.healthchecks.is_empty() {
        return Ok(());
    }

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
