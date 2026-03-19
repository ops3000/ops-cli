use crate::types::{OpsToml, DeployTarget, AppDef};
use crate::commands::common::resolve_env_value;
use crate::commands::ssh::SshSession;
use crate::commands::scp;
use crate::{api, config, prompt};
use anyhow::{anyhow, bail, Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

/// 读取并解析 ops.toml
pub fn load_ops_toml(path: &str) -> Result<OpsToml> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Cannot read {}", path))?;
    let config: OpsToml = toml::from_str(&content)
        .with_context(|| format!("Invalid ops.toml format in {}", path))?;
    Ok(config)
}

// ===== 辅助函数 =====

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

/// Resolve app name: first [[apps]] entry, otherwise project name
fn resolve_app_name(config: &OpsToml) -> String {
    config.apps.first()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| config.project.clone())
}

/// Resolve deploy targets from API
async fn resolve_targets(config: &OpsToml, app_filter: &Option<String>) -> Result<Vec<DeployTarget>> {
    let project = &config.project;

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    // If --app specified or apps defined, use app deploy targets API
    if let Some(app_name) = app_filter.as_ref() {
        let resp = api::get_app_deploy_targets(&token, project, app_name).await
            .with_context(|| format!("Failed to get deploy targets for '{}' in project '{}'", app_name, project))?;
        if resp.targets.is_empty() {
            return Err(anyhow!("No nodes bound to app '{}' in project '{}'", app_name, project));
        }
        return Ok(resp.targets);
    }

    // Try first app from config, otherwise use project name
    let app_name = resolve_app_name(config);
    let resp = api::get_app_deploy_targets(&token, project, &app_name).await;
    if let Ok(resp) = resp {
        if !resp.targets.is_empty() {
            return Ok(resp.targets);
        }
    }

    // Fallback: list all nodes bound to this project
    let nodes = api::list_nodes(&token).await?;
    let mut is_first = true;
    let targets: Vec<DeployTarget> = nodes.nodes.iter()
        .filter(|n| n.bound_apps.as_ref().map_or(false, |apps|
            apps.iter().any(|a| a.project_name == *project)))
        .map(|n| {
            let primary = is_first;
            is_first = false;
            DeployTarget {
                node_id: n.id,
                domain: n.domain.clone(),
                ip_address: n.ip_address.clone(),
                hostname: n.hostname.clone(),
                region: n.region.clone(),
                zone: n.zone.clone(),
                weight: 100,
                is_primary: primary,
                status: n.status.clone(),
            }
        })
        .collect();

    if targets.is_empty() {
        return Err(anyhow!("No nodes bound to project '{}'. Bind a node first with `ops set <app.project> --node <id>`.", project));
    }

    Ok(targets)
}

/// 当没有绑定节点时，交互式让用户选择一个节点并自动绑定
async fn auto_allocate_node(
    config: &OpsToml,
    app_filter: &Option<String>,
    interactive: bool,
) -> Result<Vec<DeployTarget>> {
    if !interactive {
        bail!("No nodes bound. Use `ops set <app.project> --node <id>` to bind a node first.");
    }

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    let res = api::list_nodes(&token).await?;
    if res.nodes.is_empty() {
        bail!("No nodes available. Initialize one with `ops init` first.");
    }

    // 构建选项列表
    let options: Vec<String> = res.nodes.iter().map(|n| {
        let name = n.hostname.as_deref().unwrap_or(&n.ip_address);
        let region = n.region.as_deref().unwrap_or("-");
        format!("#{} {} ({}) [{}]", n.id, name, region, n.status)
    }).collect();
    let option_refs: Vec<&str> = options.iter().map(|s| s.as_str()).collect();

    o_warn!("No nodes bound to this app. Select a node to deploy to:");
    let choice = prompt::select("Select node", &option_refs, 0, interactive)?;
    let selected = &res.nodes[choice];

    // Resolve app and project
    let project = &config.project;
    let app_name = app_filter.as_ref()
        .cloned()
        .unwrap_or_else(|| resolve_app_name(config));

    // 自动绑定
    o_step!("Binding node #{} to {}.{}...", selected.id, app_name, project);
    let bind_result = api::bind_node_by_name(
        &token, project, &app_name,
        selected.id as u64, true, None,
    ).await?;
    o_success!("   ✔ {}", bind_result.message);

    Ok(vec![DeployTarget {
        node_id: selected.id,
        domain: selected.domain.clone(),
        ip_address: selected.ip_address.clone(),
        hostname: selected.hostname.clone(),
        region: selected.region.clone(),
        zone: selected.zone.clone(),
        weight: 100,
        is_primary: true,
        status: selected.status.clone(),
    }])
}

/// ops deploy 主入口
pub async fn handle_deploy(
    file: String,
    service_filter: Option<String>,
    app_filter: Option<String>,
    restart_only: bool,
    env_vars: Vec<String>,
    node_filter: Option<u64>,
    region_filter: Option<String>,
    rolling: bool,
    force: bool,
    no_pull: bool,
    init: bool,
    interactive: bool,
) -> Result<()> {
    // 1. 解析配置
    o_step!("{}", "📦 Reading ops.toml...".cyan());
    let config = load_ops_toml(&file)?;

    let app_name = resolve_app_name(&config);
    let mut targets = match resolve_targets(&config, &app_filter).await {
        Ok(t) => t,
        Err(e) if e.to_string().contains("No nodes bound") => {
            auto_allocate_node(&config, &app_filter, interactive).await?
        }
        Err(e) => return Err(e),
    };

    // 过滤目标节点
    if let Some(nid) = node_filter {
        targets.retain(|t| t.node_id == nid as i64);
        if targets.is_empty() {
            return Err(anyhow!("Node {} is not bound to this app", nid));
        }
    }
    if let Some(ref region) = region_filter {
        targets.retain(|t| t.region.as_deref() == Some(region.as_str()));
        if targets.is_empty() {
            return Err(anyhow!("No nodes in region '{}' bound to this app", region));
        }
    }

    // 打印部署计划
    o_detail!("   Project: {}", app_name.green());
    if targets.len() == 1 {
        o_detail!("   Target: {}", targets[0].domain.cyan());
    } else {
        o_detail!("   Targets: {} node(s){}", targets.len().to_string().cyan(),
            if rolling { " (rolling)" } else { " (parallel)" });
        for t in &targets {
            let region_str = t.region.as_deref().unwrap_or("?");
            let primary_str = if t.is_primary { " *" } else { "" };
            o_detail!("     - {} ({}){}",
                t.domain.cyan(), region_str, primary_str);
        }
    }
    if let Some(ref app) = app_filter {
        let svcs = resolve_services(&config, &app_filter, &service_filter);
        if !svcs.is_empty() {
            o_detail!("   Group: {} → [{}]", app.yellow(), svcs);
        }
    }
    if let Some(ref svc) = service_filter {
        o_detail!("   Service: {}", svc.yellow());
    }

    // 2. 连接 + 部署前检查（紧跟 App/Target 后面输出）
    let session = SshSession::connect(&targets[0].node_id.to_string()).await?;
    let deploy_path = &config.deploy_path;
    session.exec(&format!("mkdir -p {}", deploy_path), None)?;

    if !restart_only {
        check_containers(&session, &config, &env_vars, force, interactive)?;
    }

    // 3. 同步 App 记录到后端
    let (_app_id, deployment_id) = sync_app_record(&config, &targets[0].domain).await;

    // 4. 部署到所有节点
    if targets.len() == 1 {
        let deploy_result = execute_deployment(
            &config, &session, &service_filter, &app_filter, restart_only, &env_vars, no_pull, init, deployment_id,
        ).await;

        if let Some(deployment_id) = deployment_id {
            update_deployment_status(deployment_id, &deploy_result).await;
        }

        deploy_result?;
        o_result!("\n{} Deployed {} to {}", "✅".green(), app_name.green(), targets[0].domain.cyan());
    } else if rolling {
        // 滚动部署：顺序执行
        let total = targets.len();
        let mut success_count = 0;
        let mut failed: Vec<String> = Vec::new();

        for (i, t) in targets.iter().enumerate() {
            let region_str = t.region.as_deref().unwrap_or("?");
            o_step!("\n{} [{}/{}] Deploying to {} ({})...",
                "🚀".cyan(), i + 1, total, t.domain.cyan(), region_str);

            let deploy_path = &config.deploy_path;
            let session = match SshSession::connect(&t.node_id.to_string()).await {
                Ok(s) => s,
                Err(e) => {
                    o_error!("   {} {} ({}): {}", "✘".red(), t.domain, region_str, e);
                    failed.push(t.domain.clone());
                    continue;
                }
            };

            if let Err(e) = session.exec(&format!("mkdir -p {}", deploy_path), None) {
                o_error!("   {} {} ({}): {}", "✘".red(), t.domain, region_str, e);
                failed.push(t.domain.clone());
                continue;
            }

            match execute_deployment(&config, &session, &service_filter, &app_filter, restart_only, &env_vars, no_pull, init, deployment_id).await {
                Ok(_) => {
                    o_success!("   {} {} ({})", "✔".green(), t.domain.green(), region_str);
                    success_count += 1;
                }
                Err(e) => {
                    o_error!("   {} {} ({}): {}", "✘".red(), t.domain, region_str, e);
                    failed.push(t.domain.clone());
                }
            }
        }

        print_deploy_summary(&app_name, success_count, &failed, deployment_id).await;
        if !failed.is_empty() {
            return Err(anyhow!("{} node(s) failed deployment", failed.len()));
        }
    } else {
        // 并行部署
        let total = targets.len();
        o_step!("\n{} Deploying to {} nodes in parallel...", "🚀".cyan(), total);

        let mut join_set = tokio::task::JoinSet::new();

        for t in targets {
            let config = config.clone();
            let sf = service_filter.clone();
            let af = app_filter.clone();
            let ev = env_vars.clone();
            let domain = t.domain.clone();
            let region = t.region.clone();
            let node_id = t.node_id;

            join_set.spawn(async move {
                let deploy_path = &config.deploy_path;
                let session = match SshSession::connect(&node_id.to_string()).await {
                    Ok(s) => s,
                    Err(e) => return (domain, region, Err(e)),
                };
                if let Err(e) = session.exec(&format!("mkdir -p {}", deploy_path), None) {
                    return (domain.clone(), region, Err(e.into()));
                }
                let result = execute_deployment(&config, &session, &sf, &af, restart_only, &ev, no_pull, init, deployment_id).await;
                (domain, region, result)
            });
        }

        let mut success_count = 0;
        let mut failed: Vec<String> = Vec::new();

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok((domain, region, deploy_result)) => {
                    let region_str = region.as_deref().unwrap_or("?");
                    match deploy_result {
                        Ok(_) => {
                            o_success!("   {} {} ({})", "✔".green(), domain.green(), region_str);
                            success_count += 1;
                        }
                        Err(e) => {
                            o_error!("   {} {} ({}): {}", "✘".red(), domain, region_str, e);
                            failed.push(domain);
                        }
                    }
                }
                Err(e) => {
                    o_error!("   {} join error: {}", "✘".red(), e);
                    failed.push("unknown".to_string());
                }
            }
        }

        print_deploy_summary(&app_name, success_count, &failed, deployment_id).await;
        if !failed.is_empty() {
            return Err(anyhow!("{} node(s) failed deployment", failed.len()));
        }
    }

    Ok(())
}

/// 打印部署汇总并更新状态
async fn print_deploy_summary(app_name: &str, success_count: usize, failed: &[String], deployment_id: Option<i64>) {
    let total = success_count + failed.len();
    if failed.is_empty() {
        o_result!("\n{} Deployed {} to {}/{} nodes",
            "✅".green(), app_name.green(), success_count, total);
    } else {
        o_result!("\n{} Deployed {} to {}/{} nodes ({} failed)",
            "⚠️".yellow(), app_name.yellow(),
            success_count, total, failed.len());
    }

    if let Some(did) = deployment_id {
        let _status = if failed.is_empty() { "success" } else if success_count > 0 { "partial" } else { "failed" };
        let result: Result<()> = if failed.is_empty() { Ok(()) } else {
            Err(anyhow!("{} node(s) failed", failed.len()))
        };
        update_deployment_status(did, &result).await;
    }
}

/// 同步 App 记录到后端，返回 (app_id, deployment_id)
async fn sync_app_record(config: &OpsToml, _target: &str) -> (Option<i64>, Option<i64>) {
    // 尝试加载 token
    let cfg = match config::load_config() {
        Ok(c) => c,
        Err(_) => {
            o_warn!("   {} (not logged in, skipping)", "⚠ App record sync skipped".yellow());
            return (None, None);
        }
    };

    let token = match cfg.token {
        Some(t) => t,
        None => {
            o_warn!("   {} (not logged in, skipping)", "⚠ App record sync skipped".yellow());
            return (None, None);
        }
    };

    // 同步 App
    o_step!("{}", "📝 Syncing app record...".cyan());
    let sync_result = match api::sync_app(&token, config).await {
        Ok(r) => r,
        Err(e) => {
            o_warn!("   {} {} (continuing anyway)", "⚠ Sync failed:".yellow(), e);
            return (None, None);
        }
    };

    let action = if sync_result.created { "Created" } else { "Updated" };
    o_success!("   ✔ {} app (ID: {})", action.green(), sync_result.app_id);

    // 创建部署记录
    let deployment = match api::create_deployment(&token, sync_result.app_id, "cli").await {
        Ok(d) => d,
        Err(e) => {
            o_warn!("   {} {} (continuing anyway)", "⚠ Deployment record failed:".yellow(), e);
            return (Some(sync_result.app_id), None);
        }
    };

    o_success!("   ✔ Deployment #{} started", deployment.id);

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
            o_warn!("   {} {}", "⚠ Failed to update deployment status:".yellow(), e);
        }
    }
}

/// 执行实际部署流程
async fn execute_deployment(
    config: &OpsToml,
    session: &SshSession,
    service_filter: &Option<String>,
    app_filter: &Option<String>,
    restart_only: bool,
    env_vars: &[String],
    no_pull: bool,
    init: bool,
    deployment_id: Option<i64>,
) -> Result<()> {
    sync_env_files(config, session)?;
    sync_directories(config, session).await?;

    if !restart_only {
        sync_code(config, session, app_filter, service_filter, env_vars)?;
    }

    let deploy_path = &config.deploy_path;
    let project = &config.project;
    let env = env_prefix(env_vars);
    let compose_arg = {
        let compose = compose_file_args(config);
        if compose.is_empty() { String::new() } else { format!(" {}", compose) }
    };

    // Collect app services vs infra services
    let app_svcs = collect_app_services(config);
    let infra_svcs = collect_infra_services(config, session, env_vars)?;

    // Start infra (shared, not versioned)
    if !infra_svcs.is_empty() && !restart_only {
        let infra_list = infra_svcs.join(" ");
        o_step!("\n{}", "🔧 Ensuring infrastructure...".cyan());
        let cmd = format!(
            "cd {} && {}docker compose -p {} {} up -d --no-deps {}",
            deploy_path, env, project, compose_arg.trim(), infra_list
        );
        session.exec(&cmd, None)?;
    }

    // Deploy each app with deploy-id
    let apps_with_port: Vec<_> = config.apps.iter()
        .filter(|a| a.port.is_some())
        .filter(|a| app_filter.is_none() || app_filter.as_ref() == Some(&a.name))
        .collect();

    if let Some(did) = deployment_id {
        if !restart_only && !apps_with_port.is_empty() {
            for app in &apps_with_port {
                deploy_app_zero_downtime(config, session, did, app, &env, &compose_arg, no_pull)?;
            }

            if init {
                // Run init on new containers
                for step in &config.init {
                    if app_svcs.contains(&step.service) {
                        for command in step.all_commands() {
                            // Find the new container name
                            let container = format!("{}-{}-{}", project, step.service, did);
                            o_detail!("   {} → {}", step.service.yellow(), command);
                            session.exec(&format!("docker exec {} {}", container, command), None)?;
                        }
                    }
                }
            }

            return Ok(());
        }
    }

    // Fallback: traditional build + up (for restart_only or no deployment_id)
    build_and_start(config, session, service_filter, app_filter, restart_only, env_vars, no_pull)?;

    if init {
        run_init_commands(config, session, env_vars)?;
    }

    if !restart_only {
        upload_caddy_routes(config, session, app_filter)?;
    }

    run_health_checks(config, session)?;

    Ok(())
}

/// Zero-downtime deploy: start new container with deploy_id, health check, switch Caddy, stop old
fn deploy_app_zero_downtime(
    config: &OpsToml,
    session: &SshSession,
    deployment_id: i64,
    app: &AppDef,
    env: &str,
    compose_arg: &str,
    no_pull: bool,
) -> Result<()> {
    let project = &config.project;
    let deploy_path = &config.deploy_path;
    let port = app.port.unwrap();
    let active_file = format!("{}/.ops-active-deployment", deploy_path);
    let svc_list: String = app.services.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ");

    // 1. Build image
    o_step!("\n{}", "🔨 Building images...".cyan());
    let pull_arg = if no_pull { "" } else { " --pull" };
    let build_cmd = format!(
        "cd {} && {}docker compose -p {} {} build{} {}",
        deploy_path, env, project, compose_arg.trim(), pull_arg, svc_list
    );
    session.exec(&build_cmd, None)?;

    for svc in &app.services {
        let image = format!("{}-{}:latest", project, svc);
        let new_name = format!("{}-{}-{}", project, svc, deployment_id);

        // 2. Detect network
        let network = detect_network(session, project)?;

        // 3. Generate env file from compose config
        let env_file = format!("{}/.ops-env-{}", deploy_path, svc);
        let gen_env_cmd = format!(
            "cd {} && docker compose config --format json 2>/dev/null | python3 -c \"import sys,json; svc=json.load(sys.stdin)['services'].get('{}',{{}}); [print(f'{{{{k}}}}={{{{v}}}}') for k,v in svc.get('environment',{{}}).items()]\" > {} 2>/dev/null; cat {}",
            deploy_path, svc, env_file, env_file
        );
        let env_out = session.exec_output(&gen_env_cmd).unwrap_or_default();
        let env_content = String::from_utf8_lossy(&env_out).trim().to_string();
        o_debug!("   env: {} lines", env_content.lines().count());

        // 4. Start new container
        o_step!("\n{}", format!("🚀 Starting {}", new_name).cyan());
        let volumes = format!("{}/public:/app/public", deploy_path);
        let run_cmd = format!(
            "docker run -d --name {} --network {} --env-file {} -v {} {}",
            new_name, network, env_file, volumes, image
        );
        session.exec(&run_cmd, None)?;

        // 5. Resolve IP
        let ip = resolve_container_ip(session, &new_name)?;
        o_detail!("   {} → {}:{}", new_name.cyan(), ip, port);

        // 6. Health check
        o_step!("\n{}", "💚 Health check...".cyan());
        let hc = config.healthchecks.iter().find(|h| h.name == app.name);
        let health_path = hc
            .map(|h| {
                // Extract path from URL: "https://example.com/api/v1/health" -> "/api/v1/health"
                h.url.splitn(4, '/').nth(3).map(|p| format!("/{}", p)).unwrap_or_else(|| "/status".into())
            })
            .unwrap_or_else(|| "/status".into());
        let retries = hc.map(|h| h.retries).unwrap_or(10);
        let interval = hc.map(|h| h.interval).unwrap_or(2);
        let initial_delay = hc.map(|h| h.initial_delay).unwrap_or(0);
        let health_url = format!("http://{}:{}{}", ip, port, health_path);
        o_detail!("   url: {}  retries: {}  interval: {}s  delay: {}s", health_url, retries, interval, initial_delay);
        let delay_cmd = if initial_delay > 0 { format!("sleep {}; ", initial_delay) } else { String::new() };
        let seq = (1..=retries).map(|i| i.to_string()).collect::<Vec<_>>().join(" ");
        let health_cmd = format!(
            "{}for i in {}; do curl -sf {} > /dev/null && echo 'OK' && exit 0; sleep {}; done; echo 'FAIL'; exit 1",
            delay_cmd, seq, health_url, interval
        );
        if let Err(_) = session.exec(&health_cmd, None) {
            o_warn!("   {} Health check failed, rolling back", "✘".red());
            session.exec(&format!("docker rm -f {}", new_name), None)?;
            return Err(anyhow::anyhow!("Health check failed for {}", new_name));
        }
        o_success!("   {} Healthy", "✔".green());

        // 7. Switch Caddy routes
        o_step!("\n{}", "⚙️  Switching routes...".cyan());
        upload_caddy_routes_for_app(session, config, app, &ip, port)?;

        // 8. Stop old container
        let old_id = session.exec_output(&format!("cat {} 2>/dev/null", active_file))
            .map(|o| String::from_utf8_lossy(&o).trim().to_string())
            .unwrap_or_default();

        if !old_id.is_empty() && old_id != deployment_id.to_string() {
            let old_name = format!("{}-{}-{}", project, svc, old_id);
            o_step!("{}", format!("🛑 Stopping old {}", old_name).cyan());
            let _ = session.exec(&format!("docker rm -f {}", old_name), None);
        }

        // Also clean up any legacy blue-green containers
        let _ = session.exec(&format!("rm -f {}/.ops-slot", deploy_path), None);
    }

    // 9. Write active deployment
    session.exec(&format!("echo {} > {}", deployment_id, active_file), None)?;
    o_detail!("   Active deployment: {}", deployment_id.to_string().green());

    // 10. Prune
    session.exec("docker image prune -f", None)?;

    Ok(())
}

fn detect_network(session: &SshSession, project: &str) -> Result<String> {
    // Try common network names
    let candidates = [
        format!("{}-net", project),
        format!("{}_default", project),
        "judge-net".to_string(),
    ];
    for net in &candidates {
        let check = session.exec_output(&format!("docker network inspect {} 2>/dev/null && echo OK", net));
        if let Ok(out) = check {
            if String::from_utf8_lossy(&out).contains("OK") {
                return Ok(net.clone());
            }
        }
    }
    // Fallback: look for any network containing project name
    let out = session.exec_output(&format!(
        "docker network ls --format '{{{{.Name}}}}' | grep -i {} | head -1",
        project
    )).unwrap_or_default();
    let net = String::from_utf8_lossy(&out).trim().to_string();
    if net.is_empty() {
        Err(anyhow::anyhow!("No Docker network found for project {}", project))
    } else {
        Ok(net)
    }
}

fn resolve_container_ip(session: &SshSession, container_name: &str) -> Result<String> {
    let cmd = format!(
        "docker inspect -f '{{{{range .NetworkSettings.Networks}}}}{{{{.IPAddress}}}}{{{{end}}}}' {}",
        container_name
    );
    let out = session.exec_output(&cmd)?;
    let ip = String::from_utf8_lossy(&out).trim().to_string();
    if ip.is_empty() {
        Err(anyhow::anyhow!("Failed to resolve IP for {}", container_name))
    } else {
        Ok(ip)
    }
}

fn build_container_env_file(session: &SshSession, deploy_path: &str, env: &str, project: &str, compose_arg: &str, svc: &str) -> Result<String> {
    // Extract environment variables from compose config into a temp env file
    let env_file = format!("{}/.ops-env-{}", deploy_path, svc);
    let cmd = format!(
        "cd {} && {}docker compose -p {} {} config --format json 2>/dev/null | python3 -c \"import sys,json; svc=json.load(sys.stdin)['services'].get('{}',{{}}); [print(f'{{k}}={{v}}') for k,v in svc.get('environment',{{}}).items()]\" > {} 2>/dev/null || touch {}",
        deploy_path, env, project, compose_arg.trim(), svc, env_file, env_file
    );
    session.exec(&cmd, None)?;
    Ok(env_file)
}

fn upload_caddy_routes_for_app(session: &SshSession, config: &OpsToml, app: &AppDef, ip: &str, port: u16) -> Result<()> {
    let project = &config.project;
    let target = format!("{}.{}", app.name, project);
    let conf_name = format!("ops-{}-{}", app.name, project);

    let mut caddy_content = String::new();

    // App target route
    let matcher = conf_name.replace('-', "_");
    caddy_content.push_str(&format!(
        "# {}\n@{} header X-OPS-Target {}\nhandle @{} {{\n    reverse_proxy {}:{}\n}}\n",
        target, matcher, target, matcher, ip, port
    ));

    // Domain routes
    for route in &config.routes {
        let domain = &route.domain;
        let route_matcher = domain.replace('.', "_").replace('-', "_");
        caddy_content.push_str(&format!(
            "\n# {}\n@{} host {}\nhandle @{} {{\n    reverse_proxy {}:{}\n}}\n",
            domain, route_matcher, domain, route_matcher, ip, port
        ));
        o_detail!("   ✔ {} → {}:{}", domain.cyan(), ip, port);
    }

    // Write and reload
    let caddy_path = format!("/etc/caddy/routes.d/{}.caddy", conf_name);
    session.exec(
        &format!("mkdir -p /etc/caddy/routes.d && cat > {}", caddy_path),
        Some(&caddy_content),
    )?;

    let validate = session.exec("caddy validate --config /etc/caddy/Caddyfile", None);
    if validate.is_ok() {
        session.exec("systemctl reload caddy", None)?;
        o_success!("   ✔ Caddy reloaded");
    } else {
        o_warn!("   {} Caddy validation failed", "⚠".yellow());
    }

    Ok(())
}

// ===== 内部函数 =====

/// 上传 deploy key 到服务器，按项目隔离: ~/.ssh/{project_name}/{key_filename}
fn setup_deploy_key(session: &SshSession, local_key_path: &str, project_name: &str) -> Result<()> {
    let key_content = fs::read_to_string(local_key_path)
        .with_context(|| format!("Cannot read deploy key: {}", local_key_path))?;

    let key_filename = Path::new(local_key_path)
        .file_name()
        .context("Invalid key path")?
        .to_str()
        .context("Invalid key filename")?;

    let remote_key_dir = format!("~/.ssh/{}", project_name);
    let remote_key_path = format!("{}/{}", remote_key_dir, key_filename);

    session.exec(
        &format!("mkdir -p {} && cat > {} && chmod 600 {}", remote_key_dir, remote_key_path, remote_key_path),
        Some(&key_content),
    )?;

    session.exec(
        &format!(
            r#"grep -q '{}' ~/.ssh/config 2>/dev/null || cat >> ~/.ssh/config << 'SSHEOF'
Host github.com
  Hostname ssh.github.com
  Port 443
  User git
  IdentityFile {}
  IdentitiesOnly yes
  StrictHostKeyChecking no
SSHEOF
chmod 600 ~/.ssh/config"#,
            remote_key_path, remote_key_path
        ),
        None,
    )?;

    o_success!("   {} ({})", "✔ Deploy key configured".green(), remote_key_path);
    Ok(())
}

fn sync_code(
    config: &OpsToml,
    session: &SshSession,
    app_filter: &Option<String>,
    service_filter: &Option<String>,
    env_vars: &[String],
) -> Result<()> {
    let deploy_path = &config.deploy_path;

    match config.deploy.source.as_str() {
        "git" => {
            o_step!("\n{}", "📤 Syncing code (git)...".cyan());
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
            let output = session.exec_output(&check)?;
            let output_str = String::from_utf8_lossy(&output).trim().to_string();

            if output_str == "exists" {
                let cmd = format!("cd {} && git pull origin {}", deploy_path, branch);
                session.exec(&cmd, None)?;
            } else {
                // 初次 clone — 先配置 deploy key
                if let Some(key_path) = &git.ssh_key {
                    let expanded = shellexpand::tilde(key_path).to_string();
                    let project_name = resolve_app_name(config);
                    setup_deploy_key(session, &expanded, &project_name)?;
                }
                let cmd = format!(
                    "GIT_SSH_COMMAND='ssh -o StrictHostKeyChecking=no' git clone -b {} {} {}",
                    branch, git.repo, deploy_path
                );
                session.exec(&cmd, None)?;
            }
            o_success!("   {}", "✔ Code synced.".green());
        }
        "push" => {
            o_step!("\n{}", "📤 Syncing code (rsync)...".cyan());
            session.rsync_push(&deploy_path, &config.deploy.include)?;
            o_success!("   {}", "✔ Code synced.".green());
        }
        "image" => {
            o_step!("\n{}", "🐳 Pulling images...".cyan());

            // 1. Docker login
            if let Some(reg) = &config.deploy.registry {
                let user = resolve_env_value(&reg.username)?;
                let token = resolve_env_value(&reg.token)?;
                session.exec(
                    &format!("echo '{}' | docker login {} -u {} --password-stdin", token, reg.url, user),
                    None,
                )?;
                o_success!("   {}", "✔ Registry login".green());
            }

            // 2. Pull
            let compose = compose_file_args(config);
            let env = env_prefix(env_vars);
            let svcs = resolve_services(config, app_filter, service_filter);
            let cmd = format!("cd {} && {}docker compose {} pull {}", deploy_path, env, compose, svcs);
            session.exec(&cmd, None)?;
            o_success!("   {}", "✔ Images pulled".green());
        }
        other => return Err(anyhow::anyhow!("Unknown deploy source: {}", other)),
    }
    Ok(())
}

fn sync_env_files(config: &OpsToml, session: &SshSession) -> Result<()> {
    if config.env_files.is_empty() {
        return Ok(());
    }

    let deploy_path = &config.deploy_path;
    let mut printed_header = false;

    for ef in &config.env_files {
        if Path::new(&ef.local).exists() {
            if !printed_header {
                o_step!("\n{}", "📤 Syncing env files...".cyan());
                printed_header = true;
            }
            let content = fs::read_to_string(&ef.local)?;
            let remote_path = format!("{}/{}", deploy_path, ef.remote);
            // Ensure parent directory exists
            session.exec(&format!("mkdir -p $(dirname {})", remote_path), None)?;
            session.exec(
                &format!("cat > {}", remote_path),
                Some(&content),
            )?;
            o_detail!("   ✔ {} → {}", ef.local.cyan(), remote_path);
        }
    }
    Ok(())
}

async fn sync_directories(config: &OpsToml, session: &SshSession) -> Result<()> {
    if config.sync.is_empty() {
        return Ok(());
    }

    let deploy_path = &config.deploy_path;
    let target = session.target();
    let mut printed_header = false;

    for s in &config.sync {
        if Path::new(&s.local).exists() {
            if !printed_header {
                o_step!("\n{}", "📤 Syncing directories...".cyan());
                printed_header = true;
            }
            let remote = format!("{}:{}/{}", target, deploy_path, s.remote);
            o_detail!("   {} → {}", s.local.cyan(), remote);
            // Ensure parent directory exists on remote
            session.exec(&format!("mkdir -p {}/{}", deploy_path, s.remote), None)?;
            scp::handle_push(s.local.clone(), remote).await?;
        }
    }
    Ok(())
}

/// Upload Caddy route fragments for each app
fn upload_caddy_routes(config: &OpsToml, session: &SshSession, app_filter: &Option<String>) -> Result<()> {
    let project_name = &config.project;

    // Ensure routes directory exists
    session.exec("mkdir -p /etc/caddy/routes.d", None)?;

    // Collect app → port mappings from [[routes]] (legacy) and [[apps]]
    let mut routes_written = false;

    // Handle legacy [[routes]]
    if !config.routes.is_empty() {
        let deployed_app = app_filter.as_ref()
            .cloned()
            .unwrap_or_else(|| resolve_app_name(config));

        o_step!("\n{}", "⚙️  Generating Caddy routes...".cyan());

        // Group routes by port to determine if we need domain-based matching
        let first_port = config.routes[0].port;
        let all_same_port = config.routes.iter().all(|r| r.port == first_port);

        if all_same_port {
            // All routes share the same port — use X-OPS-Target matcher
            let target = format!("{}.{}", deployed_app, project_name);
            let matcher_name = format!("ops_{}_{}", deployed_app, project_name).replace('-', "_");
            let caddy_snippet = format!(
                "# {target}\n@{matcher} header X-OPS-Target {target}\nhandle @{matcher} {{\n    reverse_proxy 127.0.0.1:{port}\n}}\n",
                target = target,
                matcher = matcher_name,
                port = first_port,
            );
            let conf_name = format!("ops-{}-{}.caddy", deployed_app, project_name);
            session.exec(
                &format!("cat > /etc/caddy/routes.d/{}", conf_name),
                Some(&caddy_snippet),
            )?;
            o_detail!("   ✔ {} → :{}", target.green(), first_port);
        } else {
            // Different ports per route — use X-Forwarded-Host for domain-based matching
            for route in &config.routes {
                let safe_domain = route.domain.replace('.', "_").replace('-', "_");
                let matcher_name = format!("ops_route_{}", safe_domain);
                let caddy_snippet = format!(
                    "# {domain}\n@{matcher} header X-Forwarded-Host {domain}\nhandle @{matcher} {{\n    reverse_proxy 127.0.0.1:{port}\n}}\n",
                    domain = route.domain,
                    matcher = matcher_name,
                    port = route.port,
                );
                let conf_name = format!("ops-route-{}.caddy", safe_domain);
                session.exec(
                    &format!("cat > /etc/caddy/routes.d/{}", conf_name),
                    Some(&caddy_snippet),
                )?;
                o_detail!("   ✔ {} → :{}", route.domain.green(), route.port);
            }
        }
        routes_written = true;
    }

    // Handle [[apps]] with port (skip if [[routes]] already covered them)
    let route_ports: std::collections::HashSet<u16> = config.routes.iter().map(|r| r.port).collect();
    let apps_to_process: Vec<_> = if let Some(ref filter) = app_filter {
        config.apps.iter().filter(|a| a.name == *filter).collect()
    } else {
        config.apps.iter().collect()
    };
    let apps_with_port: Vec<_> = apps_to_process.iter()
        .filter(|a| a.port.is_some() && !route_ports.contains(&a.port.unwrap()))
        .collect();

    if !apps_with_port.is_empty() {
        if !routes_written {
            o_step!("\n{}", "⚙️  Generating Caddy routes...".cyan());
        }

        for app in &apps_with_port {
            let port = app.port.unwrap();
            let target = format!("{}.{}", app.name, project_name);
            let matcher_name = format!("ops_{}_{}", app.name, project_name).replace('-', "_");
            let caddy_snippet = format!(
                "# {target}\n@{matcher} header X-OPS-Target {target}\nhandle @{matcher} {{\n    reverse_proxy 127.0.0.1:{port}\n}}\n",
                target = target,
                matcher = matcher_name,
                port = port,
            );
            let conf_name = format!("ops-{}-{}.caddy", app.name, project_name);
            session.exec(
                &format!("cat > /etc/caddy/routes.d/{}", conf_name),
                Some(&caddy_snippet),
            )?;
            o_detail!("   ✔ {} → :{}", target.green(), port);
        }
        routes_written = true;
    }

    if routes_written {
        // Validate & reload Caddy
        session.exec("caddy validate --config /etc/caddy/Caddyfile && systemctl reload caddy", None)?;
    }

    Ok(())
}

/// 部署前检查：展示将要部署的 services 和远程现有容器，询问用户操作
fn check_containers(
    session: &SshSession,
    config: &OpsToml,
    env_vars: &[String],
    force: bool,
    interactive: bool,
) -> Result<()> {
    let deploy_path = &config.deploy_path;
    let compose = compose_file_args(config);
    let env = env_prefix(env_vars);
    let compose_arg = if compose.is_empty() { String::new() } else { format!(" {}", compose) };

    // 1. 列出将要部署的 services
    let services_cmd = format!(
        "cd {} && {}docker compose{} config --services 2>/dev/null",
        deploy_path, env, compose_arg
    );
    let services_output = session.exec_output(&services_cmd).unwrap_or_default();
    let services_str = String::from_utf8_lossy(&services_output);
    let services: Vec<&str> = services_str.trim().lines().collect();

    if !services.is_empty() {
        if !config.apps.is_empty() {
            // 有 app 分组 → 按组显示
            o_detail!("   Apps:");
            let mut grouped = std::collections::HashSet::new();
            for app in &config.apps {
                let svcs = app.services.join(", ");
                o_detail!("     {} → [{}]", app.name.yellow(), svcs.cyan());
                for s in &app.services {
                    grouped.insert(s.as_str());
                }
            }
            let ungrouped: Vec<&str> = services.iter()
                .filter(|s| !grouped.contains(*s))
                .copied()
                .collect();
            if !ungrouped.is_empty() {
                o_detail!("   Ungrouped: {}", ungrouped.join(", ").dimmed());
            }
        } else {
            // 没有分组 → 扁平列表
            o_detail!("   Services ({}): {}", services.len().to_string().yellow(), services.join(", ").cyan());
        }
    }

    // 2. 查询远程现有容器
    let ps_cmd = "docker ps -a --format 'table {{.Names}}\t{{.Status}}\t{{.Image}}' 2>/dev/null";
    let ps_output = session.exec_output(ps_cmd).unwrap_or_default();
    let ps_str = String::from_utf8_lossy(&ps_output).trim().to_string();

    if ps_str.is_empty() || ps_str.lines().count() <= 1 {
        // 没有容器，直接继续
        return Ok(());
    }

    o_detail!("\n{}", "📦 Existing containers on remote:".yellow());
    for line in ps_str.lines() {
        o_detail!("   {}", line);
    }

    // 3. --force 自动 clean
    if force {
        o_step!("\n   {} (--force)", "Cleaning old containers...".yellow());
        let down_cmd = format!(
            "cd {} && {}docker compose{} down --remove-orphans 2>/dev/null; true",
            deploy_path, env, compose_arg
        );
        session.exec(&down_cmd, None)?;
        o_success!("   {}", "✔ Old containers removed".green());
        return Ok(());
    }

    // 4. 交互式询问（非交互模式自动选择默认：继续部署）
    let options = &[
        "Continue deploy",
        "Clean & Deploy (docker compose down first)",
        "Abort",
    ];
    let choice = prompt::select("Select action", options, 0, interactive)?;

    match choice {
        1 => {
            o_step!("\n   {}", "Cleaning old containers...".yellow());
            let down_cmd = format!(
                "cd {} && {}docker compose{} down --remove-orphans 2>/dev/null; true",
                deploy_path, env, compose_arg
            );
            session.exec(&down_cmd, None)?;
            o_success!("   {}", "✔ Old containers removed".green());
            Ok(())
        }
        2 => Err(anyhow!("Deployment aborted by user")),
        _ => Ok(()), // 0 = 继续部署
    }
}

fn build_and_start(
    config: &OpsToml,
    session: &SshSession,
    service_filter: &Option<String>,
    app_filter: &Option<String>,
    restart_only: bool,
    env_vars: &[String],
    no_pull: bool,
) -> Result<()> {
    let deploy_path = &config.deploy_path;

    let compose = compose_file_args(config);
    let env = env_prefix(env_vars);
    let svcs = resolve_services(config, app_filter, service_filter);

    // Add space before compose args and services if non-empty
    let compose_arg = if compose.is_empty() { String::new() } else { format!(" {}", compose) };
    let svc_arg = if svcs.is_empty() { String::new() } else { format!(" {}", svcs) };

    o_step!("\n{}", "🚀 Building & starting services...".cyan());

    if restart_only {
        let cmd = format!("cd {} && {}docker compose{} restart{}", deploy_path, env, compose_arg, svc_arg);
        session.exec(&cmd, None)?;
    } else if config.deploy.source == "image" {
        // image 模式: 只 up，不 build
        let cmd = format!(
            "cd {} && {}docker compose{} up -d --remove-orphans{}",
            deploy_path, env, compose_arg, svc_arg
        );
        session.exec(&cmd, None)?;
        // 清理旧镜像
        session.exec("docker image prune -f", None).ok();
    } else {
        // 旧行为: build + up
        let pull_arg = if no_pull { "" } else { " --pull" };
        let cmd = format!(
            "cd {} && {}docker compose{} build{}{} && {}docker compose{} up -d --remove-orphans{}",
            deploy_path, env, compose_arg, pull_arg, svc_arg, env, compose_arg, svc_arg
        );
        session.exec(&cmd, None)?;
    }

    Ok(())
}

// ===== Blue-Green Zero-Downtime Deploy =====

/// 获取 app services 列表（[[apps]] 中定义的 services）
fn collect_app_services(config: &OpsToml) -> Vec<String> {
    let mut svcs = Vec::new();
    for app in &config.apps {
        for s in &app.services {
            if !svcs.contains(s) {
                svcs.push(s.clone());
            }
        }
    }
    svcs
}

/// 获取基础设施 services（compose 中定义但不在 [[apps]] 中的 services）
fn collect_infra_services(config: &OpsToml, session: &SshSession, env_vars: &[String]) -> Result<Vec<String>> {
    let deploy_path = &config.deploy_path;
    let compose = compose_file_args(config);
    let env = env_prefix(env_vars);
    let compose_arg = if compose.is_empty() { String::new() } else { format!(" {}", compose) };

    let cmd = format!(
        "cd {} && {}docker compose{} config --services 2>/dev/null",
        deploy_path, env, compose_arg
    );
    let output = session.exec_output(&cmd).unwrap_or_default();
    let all_services: Vec<String> = String::from_utf8_lossy(&output)
        .trim()
        .lines()
        .map(|s| s.to_string())
        .collect();

    let app_svcs = collect_app_services(config);
    Ok(all_services.into_iter().filter(|s| !app_svcs.contains(s)).collect())
}

/// Blue-green 零停机部署
fn blue_green_deploy(
    config: &OpsToml,
    session: &SshSession,
    service_filter: &Option<String>,
    app_filter: &Option<String>,
    env_vars: &[String],
    no_pull: bool,
    init: bool,
) -> Result<()> {
    let deploy_path = &config.deploy_path;
    let compose = compose_file_args(config);
    let env = env_prefix(env_vars);
    let compose_arg = if compose.is_empty() { String::new() } else { format!(" {}", compose) };
    let project = &config.project;

    // 1. 读取当前 slot
    let slot_file = format!("{}/.ops-slot", deploy_path);
    let active_slot = session.exec_output(&format!("cat {} 2>/dev/null || echo blue", slot_file))
        .map(|o| String::from_utf8_lossy(&o).trim().to_string())
        .unwrap_or_else(|_| "blue".to_string());
    let target_slot = if active_slot == "blue" { "green" } else { "blue" };

    o_step!("\n{}", format!("🔄 Blue-Green Deploy: {} → {}", active_slot, target_slot).cyan());

    // 2. 确保基础设施在主 project 中运行
    let infra_svcs = collect_infra_services(config, session, env_vars)?;
    if !infra_svcs.is_empty() {
        let infra_list = infra_svcs.join(" ");
        o_detail!("   Ensuring infra: {}", infra_list.dimmed());
        let cmd = format!(
            "cd {} && {}docker compose -p {} {} up -d --no-deps {}",
            deploy_path, env, project, compose_arg.trim(), infra_list
        );
        session.exec(&cmd, None)?;
    }

    // 3. 确定要部署的 app services
    let app_svcs = if let Some(ref filter) = service_filter {
        vec![filter.clone()]
    } else if let Some(ref app_name) = app_filter {
        config.apps.iter()
            .find(|a| a.name == *app_name)
            .map(|a| a.services.clone())
            .unwrap_or_else(|| collect_app_services(config))
    } else {
        collect_app_services(config)
    };
    let svc_list = app_svcs.join(" ");

    // 4. 构建新镜像 (在主 project 下构建，镜像名可复用)
    o_step!("\n{}", "🔨 Building images...".cyan());
    let pull_arg = if no_pull { "" } else { " --pull" };
    let build_cmd = format!(
        "cd {} && {}docker compose -p {} {}{} build --no-cache{} {}",
        deploy_path, env, project, compose_arg.trim(),
        if compose_arg.is_empty() { "" } else { " " },
        pull_arg, svc_list
    );
    session.exec(&build_cmd, None)?;

    // 5. 启动 target slot (不同 project name，共享 juglans-net 网络)
    let target_project = format!("{}-{}", project, target_slot);
    o_step!("\n{}", format!("🚀 Starting {} slot...", target_slot).cyan());
    let up_cmd = format!(
        "cd {} && {}docker compose -p {} {} up -d --no-deps {}",
        deploy_path, env, target_project, compose_arg.trim(), svc_list
    );
    session.exec(&up_cmd, None)?;

    // 6. Init commands (迁移等) — 在 target slot 的容器里运行
    if init {
        o_step!("\n{}", "🔧 Running init commands on new slot...".cyan());
        for step in &config.init {
            // 只运行属于本次部署 services 的 init
            if app_svcs.contains(&step.service) {
                for command in step.all_commands() {
                    o_detail!("   {} → {}", step.service.yellow(), command);
                    let cmd = format!(
                        "cd {} && {}docker compose -p {} {} exec {} {}",
                        deploy_path, env, target_project, compose_arg.trim(), step.service, command
                    );
                    session.exec(&cmd, None)?;
                }
                o_success!("   ✔ {}", step.service.green());
            }
        }
    }

    // 7. 获取 target slot 容器 IP
    o_step!("\n{}", "🔍 Resolving container IPs...".cyan());
    let mut ip_map: std::collections::HashMap<String, (String, u16)> = std::collections::HashMap::new();

    for app in &config.apps {
        if let Some(port) = app.port {
            for svc in &app.services {
                if !app_svcs.contains(svc) { continue; }
                let container_name = format!("{}-{}-1", target_project, svc);
                let inspect_cmd = format!(
                    "docker inspect -f '{{{{range .NetworkSettings.Networks}}}}{{{{.IPAddress}}}}{{{{end}}}}' {}",
                    container_name
                );
                let ip = session.exec_output(&inspect_cmd)
                    .map(|o| String::from_utf8_lossy(&o).trim().to_string())
                    .unwrap_or_default();

                if ip.is_empty() {
                    o_warn!("   {} Could not resolve IP for {}", "⚠".yellow(), container_name);
                    continue;
                }
                o_detail!("   {} → {}:{}", svc.cyan(), ip, port);
                ip_map.insert(app.name.clone(), (ip, port));
            }
        }
    }

    if ip_map.is_empty() {
        return Err(anyhow!("No container IPs resolved — aborting blue-green switch"));
    }

    // 8. 健康检查 (对新容器 IP)
    o_step!("\n{}", "💚 Health checks on new slot...".cyan());
    let mut all_healthy = true;
    for hc in &config.healthchecks {
        // 查找对应的 app → IP
        if let Some((ip, port)) = config.apps.iter()
            .find(|a| a.name == hc.name)
            .and_then(|a| ip_map.get(&a.name))
        {
            let health_url = format!("http://{}:{}", ip, port);
            // 用 health_url 替换原 URL 的 host:port 部分
            let path = hc.url.splitn(4, '/').skip(3).next().unwrap_or("");
            let check_url = if path.is_empty() {
                health_url
            } else {
                format!("{}/{}", health_url, path)
            };
            let cmd = format!(
                "for i in 1 2 3 4 5 6 7 8 9 10; do curl -sf {} > /dev/null && echo 'OK' && exit 0; sleep 2; done; echo 'FAIL'; exit 1",
                check_url
            );
            let output = session.exec_output(&cmd);
            match output {
                Ok(o) if String::from_utf8_lossy(&o).trim() == "OK" => {
                    o_success!("   ✔ {}  {}  {}", hc.name.green(), check_url, "OK".green());
                }
                _ => {
                    o_warn!("   ✘ {}  {}  {}", hc.name.red(), check_url, "FAILED".red());
                    all_healthy = false;
                }
            }
        }
    }

    if !all_healthy {
        // 健康检查失败 — 停掉 target slot，不切流量
        o_warn!("\n{}", "⚠ Health checks failed — rolling back (stopping new slot)".yellow());
        let down_cmd = format!(
            "cd {} && docker compose -p {} down 2>/dev/null; true",
            deploy_path, target_project
        );
        session.exec(&down_cmd, None)?;
        return Err(anyhow!("Blue-green deploy aborted: health checks failed on new slot"));
    }

    // 9. 更新 Caddy 路由 → 指向新容器 IP
    o_step!("\n{}", "⚙️  Switching Caddy routes to new slot...".cyan());
    upload_caddy_routes_bg(config, session, &ip_map)?;

    // 10. 写入新的 active slot
    session.exec(&format!("echo {} > {}", target_slot, slot_file), None)?;
    o_detail!("   Active slot: {}", target_slot.green());

    // 11. 停掉旧 slot
    let old_project = format!("{}-{}", project, active_slot);
    // 检查旧 slot 是否存在（首次部署可能没有旧 slot）
    let old_exists = session.exec_output(&format!(
        "docker compose -p {} ps -q 2>/dev/null | head -1",
        old_project
    )).map(|o| !String::from_utf8_lossy(&o).trim().is_empty()).unwrap_or(false);

    if old_exists {
        o_step!("\n{}", format!("🛑 Stopping old {} slot...", active_slot).cyan());
        let down_cmd = format!(
            "cd {} && docker compose -p {} {} down --remove-orphans 2>/dev/null; true",
            deploy_path, old_project, compose_arg.trim()
        );
        session.exec(&down_cmd, None)?;
        o_success!("   ✔ {} slot stopped", active_slot);
    }

    // 清理旧镜像
    session.exec("docker image prune -f 2>/dev/null", None).ok();

    o_result!("\n{} Blue-green deploy complete: {} → {}", "✅".green(), active_slot, target_slot.green());
    Ok(())
}

/// 生成 Caddy 路由指向容器 IP（蓝绿模式专用）
fn upload_caddy_routes_bg(
    config: &OpsToml,
    session: &SshSession,
    ip_map: &std::collections::HashMap<String, (String, u16)>,
) -> Result<()> {
    session.exec("mkdir -p /etc/caddy/routes.d", None)?;

    let project_name = &config.project;

    // [[routes]] — domain-based routing to container IPs
    if !config.routes.is_empty() {
        for route in &config.routes {
            // 找到此路由对应的 app (通过 port 匹配)
            let upstream = config.apps.iter()
                .find(|a| a.port == Some(route.port))
                .and_then(|a| ip_map.get(&a.name))
                .map(|(ip, port)| format!("{}:{}", ip, port))
                .unwrap_or_else(|| format!("127.0.0.1:{}", route.port));

            let safe_domain = route.domain.replace('.', "_").replace('-', "_");
            let matcher_name = format!("ops_route_{}", safe_domain);
            let caddy_snippet = format!(
                "# {domain}\n@{matcher} header X-Forwarded-Host {domain}\nhandle @{matcher} {{\n    reverse_proxy {upstream}\n}}\n",
                domain = route.domain,
                matcher = matcher_name,
                upstream = upstream,
            );
            let conf_name = format!("ops-route-{}.caddy", safe_domain);
            session.exec(
                &format!("cat > /etc/caddy/routes.d/{}", conf_name),
                Some(&caddy_snippet),
            )?;
            o_detail!("   ✔ {} → {}", route.domain.green(), upstream);
        }
    }

    // [[apps]] with port — X-OPS-Target routing to container IPs
    for app in &config.apps {
        if app.port.is_none() { continue; }
        if let Some((ip, port)) = ip_map.get(&app.name) {
            let target = format!("{}.{}", app.name, project_name);
            let matcher_name = format!("ops_{}_{}", app.name, project_name).replace('-', "_");
            let caddy_snippet = format!(
                "# {target}\n@{matcher} header X-OPS-Target {target}\nhandle @{matcher} {{\n    reverse_proxy {ip}:{port}\n}}\n",
                target = target,
                matcher = matcher_name,
                ip = ip,
                port = port,
            );
            let conf_name = format!("ops-{}-{}.caddy", app.name, project_name);
            session.exec(
                &format!("cat > /etc/caddy/routes.d/{}", conf_name),
                Some(&caddy_snippet),
            )?;
        }
    }

    // Validate & reload Caddy
    session.exec("caddy validate --config /etc/caddy/Caddyfile && systemctl reload caddy", None)?;
    o_success!("   ✔ Caddy reloaded");

    Ok(())
}

fn run_init_commands(config: &OpsToml, session: &SshSession, env_vars: &[String]) -> Result<()> {
    if config.init.is_empty() {
        return Ok(());
    }

    let deploy_path = &config.deploy_path;
    let compose = compose_file_args(config);
    let env = env_prefix(env_vars);
    let compose_arg = if compose.is_empty() { String::new() } else { format!(" {}", compose) };

    o_step!("\n{}", "🔧 Running init commands...".cyan());

    for step in &config.init {
        for command in step.all_commands() {
            o_detail!("   {} → {}", step.service.yellow(), command);
            let cmd = format!(
                "cd {} && {}docker compose{} exec {} {}",
                deploy_path, env, compose_arg, step.service, command
            );
            session.exec(&cmd, None)?;
        }
        o_success!("   ✔ {}", step.service.green());
    }

    Ok(())
}

fn run_health_checks(config: &OpsToml, session: &SshSession) -> Result<()> {
    if config.healthchecks.is_empty() {
        return Ok(());
    }

    o_step!("\n{}", "💚 Health checks:".cyan());

    for hc in &config.healthchecks {
        let seq = (1..=hc.retries).map(|i| i.to_string()).collect::<Vec<_>>().join(" ");
        let delay_cmd = if hc.initial_delay > 0 { format!("sleep {}; ", hc.initial_delay) } else { String::new() };
        let cmd = format!(
            "{}for i in {}; do curl -sf {} > /dev/null && echo 'OK' && exit 0; sleep {}; done; echo 'FAIL'; exit 1",
            delay_cmd, seq, hc.url, hc.interval
        );
        let output = session.exec_output(&cmd);
        match output {
            Ok(o) if String::from_utf8_lossy(&o).trim() == "OK" => {
                o_success!("   ✔ {}  {}  {}", hc.name.green(), hc.url, "OK".green());
            }
            _ => {
                o_warn!("   ✘ {}  {}  {}", hc.name.red(), hc.url, "FAILED".red());
            }
        }
    }
    Ok(())
}
