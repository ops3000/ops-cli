use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, Sse},
        IntoResponse, Json,
    },
    routing::{get, post},
    Router,
};
use colored::Colorize;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;

use crate::serve::{actions, containers, logs, metrics};
use crate::update;

#[derive(Clone)]
struct AppState {
    token: String,
    compose_dirs: Vec<String>,
}

fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if auth == format!("Bearer {}", state.token) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub async fn handle_serve(token: String, port: u16, compose_dir: String) -> Result<()> {
    let compose_dirs: Vec<String> = compose_dir.split(',').map(|s| s.trim().to_string()).collect();
    for dir in &compose_dirs {
        if !std::path::Path::new(dir).exists() {
            anyhow::bail!("Compose directory does not exist: {}", dir);
        }
    }

    let state = Arc::new(AppState {
        token,
        compose_dirs,
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/containers", get(get_containers))
        .route("/logs", get(get_logs))
        .route("/logs/stream", get(stream_logs))
        .route("/metrics", get(get_metrics))
        .route("/restart", post(restart))
        .route("/stop", post(stop))
        .route("/start", post(start))
        .route("/deploy", post(deploy))
        .route("/checkupdate", get(check_update))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    println!(
        "{} ops serve listening on {}",
        "✓".green(),
        addr.cyan()
    );

    // Spawn background task to check for updates every 5 minutes
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            match tokio::task::spawn_blocking(|| update::check_and_auto_update()).await {
                Ok(Ok(true)) => {
                    eprintln!("{}", "🔄 Updated! Restarting ops serve...".yellow());
                    // Restart via systemd
                    let _ = std::process::Command::new("systemctl")
                        .args(["restart", "ops-serve"])
                        .spawn();
                    std::process::exit(0);
                }
                _ => {}
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

pub async fn handle_install(token: String, port: u16, compose_dir: String, domain: Option<String>) -> Result<()> {
    let exe_path = std::env::current_exe()?;
    let service = format!(
        r#"[Unit]
Description=OPS Serve
After=network.target docker.service
Wants=docker.service

[Service]
Type=simple
ExecStart={} serve --token {} --port {} --compose-dir {}
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
"#,
        exe_path.display(),
        token,
        port,
        compose_dir
    );

    let service_path = "/etc/systemd/system/ops-serve.service";
    std::fs::write(service_path, service)?;

    println!("{} Wrote {}", "✓".green(), service_path);

    let cmds = [
        ("systemctl daemon-reload", "Reloaded systemd"),
        ("systemctl enable ops-serve", "Enabled ops-serve"),
        ("systemctl restart ops-serve", "Restarted ops-serve"),
    ];

    for (cmd, msg) in &cmds {
        let status = std::process::Command::new("sh")
            .args(["-c", cmd])
            .status()?;
        if status.success() {
            println!("{} {}", "✓".green(), msg);
        } else {
            eprintln!("{} Failed: {}", "✗".red(), cmd);
        }
    }

    // Configure nginx reverse proxy if domain is provided
    if let Some(domain) = domain {
        // Generate self-signed certificate for Cloudflare Full SSL mode
        let cert_dir = "/etc/nginx/ssl";
        let cert_path = format!("{}/ops-serve.crt", cert_dir);
        let key_path = format!("{}/ops-serve.key", cert_dir);

        if !std::path::Path::new(&cert_path).exists() {
            std::fs::create_dir_all(cert_dir)?;
            let ssl_cmd = format!(
                "openssl req -x509 -nodes -days 3650 -newkey rsa:2048 \
                 -keyout {} -out {} -subj '/CN=ops-serve'",
                key_path, cert_path
            );
            let status = std::process::Command::new("sh")
                .args(["-c", &ssl_cmd])
                .status()?;
            if status.success() {
                println!("{} Generated self-signed SSL certificate", "✓".green());
            } else {
                eprintln!("{} Failed to generate SSL certificate", "✗".red());
            }
        }

        let nginx_conf = format!(
            r#"server {{
    listen 80;
    listen 443 ssl;
    server_name {domain};

    ssl_certificate {cert_path};
    ssl_certificate_key {key_path};

    location / {{
        proxy_pass http://127.0.0.1:{port};
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }}
}}"#
        );

        // Try sites-enabled first, fall back to conf.d
        let nginx_path = if std::path::Path::new("/etc/nginx/sites-enabled").exists() {
            "/etc/nginx/sites-enabled/ops-serve.conf"
        } else {
            "/etc/nginx/conf.d/ops-serve.conf"
        };

        std::fs::write(nginx_path, &nginx_conf)?;
        println!("{} Wrote {}", "✓".green(), nginx_path);

        let nginx_cmds = [
            ("nginx -t", "Nginx config test passed"),
            ("systemctl reload nginx", "Reloaded nginx"),
        ];

        for (cmd, msg) in &nginx_cmds {
            let status = std::process::Command::new("sh")
                .args(["-c", cmd])
                .status()?;
            if status.success() {
                println!("{} {}", "✓".green(), msg);
            } else {
                eprintln!("{} Failed: {}", "✗".red(), cmd);
            }
        }
    }

    Ok(())
}

// --- Route handlers ---

async fn health(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    // If auth is provided, return detailed health info
    if check_auth(&state, &headers).is_ok() {
        let mut all_running = true;
        let mut container_count = 0;
        for dir in &state.compose_dirs {
            if let Ok(containers) = containers::list_containers(dir) {
                for c in &containers {
                    container_count += 1;
                    if c.state != "running" {
                        all_running = false;
                    }
                }
            }
        }
        let status = if container_count == 0 {
            "unknown"
        } else if all_running {
            "healthy"
        } else {
            "degraded"
        };
        Json(serde_json::json!({
            "status": status,
            "containers": container_count,
            "all_running": all_running,
            "version": env!("CARGO_PKG_VERSION"),
        }))
    } else {
        // Basic health check (no auth required, for liveness probes)
        Json(serde_json::json!({ "status": "ok" }))
    }
}

async fn get_containers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;
    let mut all = Vec::new();
    for dir in &state.compose_dirs {
        match containers::list_containers(dir) {
            Ok(list) => all.extend(list),
            Err(e) => eprintln!("containers error for {}: {}", dir, e),
        }
    }
    Ok(Json(serde_json::json!({ "containers": all })))
}

#[derive(Deserialize)]
struct LogsQuery {
    service: String,
    #[serde(default = "default_lines")]
    lines: u32,
}

fn default_lines() -> u32 {
    100
}

async fn get_logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<LogsQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;
    // Try each compose dir; for "all", merge from all dirs
    if q.service == "all" {
        let mut combined = String::new();
        for dir in &state.compose_dirs {
            if let Ok(output) = logs::get_logs(dir, "all", q.lines) {
                combined.push_str(&output);
            }
        }
        return Ok(Json(serde_json::json!({ "logs": combined })));
    }
    // For specific service, find which dir contains it
    for dir in &state.compose_dirs {
        if let Ok(services) = containers::list_services(dir) {
            if services.iter().any(|s| s == &q.service) {
                match logs::get_logs(dir, &q.service, q.lines) {
                    Ok(output) => return Ok(Json(serde_json::json!({ "logs": output }))),
                    Err(e) => {
                        eprintln!("logs error: {}", e);
                        return Err(StatusCode::INTERNAL_SERVER_ERROR);
                    }
                }
            }
        }
    }
    Err(StatusCode::NOT_FOUND)
}

#[derive(Deserialize)]
struct StreamQuery {
    service: String,
}

async fn stream_logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<StreamQuery>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    check_auth(&state, &headers)?;

    let (tx, rx) = tokio::sync::mpsc::channel::<String>(256);
    let service = q.service.clone();

    // Find which dir contains this service, or use first dir for "all"
    let target_dir = if service == "all" {
        state.compose_dirs[0].clone()
    } else {
        let mut found = None;
        for dir in &state.compose_dirs {
            if let Ok(services) = containers::list_services(dir) {
                if services.iter().any(|s| s == &service) {
                    found = Some(dir.clone());
                    break;
                }
            }
        }
        found.unwrap_or_else(|| state.compose_dirs[0].clone())
    };

    tokio::spawn(async move {
        if let Err(e) = logs::stream_logs(&target_dir, &service, tx).await {
            eprintln!("stream_logs error: {}", e);
        }
    });

    let stream = ReceiverStream::new(rx).map(|line| Ok(Event::default().data(line)));

    Ok(Sse::new(stream))
}

async fn get_metrics(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;
    match metrics::collect_metrics() {
        Ok(m) => Ok(Json(serde_json::to_value(m).unwrap())),
        Err(e) => {
            eprintln!("metrics error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
struct ServiceQuery {
    service: String,
}

fn find_compose_dir(state: &AppState, service: &str) -> Option<String> {
    for dir in &state.compose_dirs {
        if let Ok(services) = containers::list_services(dir) {
            if services.iter().any(|s| s == service) {
                return Some(dir.clone());
            }
        }
    }
    None
}

async fn restart(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ServiceQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;
    let dir = find_compose_dir(&state, &q.service).ok_or(StatusCode::NOT_FOUND)?;
    match actions::restart_service(&dir, &q.service) {
        Ok(r) => Ok(Json(serde_json::to_value(r).unwrap())),
        Err(e) => { eprintln!("restart error: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
    }
}

async fn stop(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ServiceQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;
    let dir = find_compose_dir(&state, &q.service).ok_or(StatusCode::NOT_FOUND)?;
    match actions::stop_service(&dir, &q.service) {
        Ok(r) => Ok(Json(serde_json::to_value(r).unwrap())),
        Err(e) => { eprintln!("stop error: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
    }
}

async fn start(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ServiceQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;
    let dir = find_compose_dir(&state, &q.service).ok_or(StatusCode::NOT_FOUND)?;
    match actions::start_service(&dir, &q.service) {
        Ok(r) => Ok(Json(serde_json::to_value(r).unwrap())),
        Err(e) => { eprintln!("start error: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
    }
}

#[derive(serde::Deserialize, Default)]
struct DeployRequest {
    deploy_path: Option<String>,
    git_repo: Option<String>,
    branch: Option<String>,
}

async fn deploy(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Option<Json<DeployRequest>>,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;

    let req = body.map(|b| b.0).unwrap_or_default();

    // If deploy_path is provided, deploy that specific app
    if let Some(deploy_path) = req.deploy_path {
        match actions::deploy_with_repo(
            &deploy_path,
            req.git_repo.as_deref(),
            req.branch.as_deref(),
        ) {
            Ok(r) => return Ok(Json(serde_json::json!({
                "success": r.success,
                "message": r.message
            }))),
            Err(e) => {
                eprintln!("deploy error for {}: {}", deploy_path, e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    // Otherwise deploy all configured compose_dirs (legacy behavior)
    let mut results = Vec::new();
    for dir in &state.compose_dirs {
        match actions::deploy(dir) {
            Ok(r) => results.push(r),
            Err(e) => { eprintln!("deploy error for {}: {}", dir, e); }
        }
    }
    let all_ok = results.iter().all(|r| r.success);
    let messages: Vec<&str> = results.iter().map(|r| r.message.as_str()).collect();
    Ok(Json(serde_json::json!({
        "success": all_ok,
        "message": messages.join("; ")
    })))
}

async fn check_update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;

    let current = env!("CARGO_PKG_VERSION").to_string();
    let latest = tokio::task::spawn_blocking(|| update::check_for_update(false))
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten();

    let update_available = latest.as_ref().map(|v| v != &current).unwrap_or(false);

    Ok(Json(serde_json::json!({
        "current_version": current,
        "latest_version": latest,
        "update_available": update_available
    })))
}
