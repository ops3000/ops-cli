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

#[derive(Clone)]
struct AppState {
    token: String,
    compose_dir: String,
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
    // Verify compose dir exists
    if !std::path::Path::new(&compose_dir).exists() {
        anyhow::bail!("Compose directory does not exist: {}", compose_dir);
    }

    let state = Arc::new(AppState {
        token,
        compose_dir,
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
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    println!(
        "{} ops serve listening on {}",
        "✓".green(),
        addr.cyan()
    );

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

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn get_containers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;
    match containers::list_containers(&state.compose_dir) {
        Ok(list) => Ok(Json(serde_json::json!({ "containers": list }))),
        Err(e) => {
            eprintln!("containers error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
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
    match logs::get_logs(&state.compose_dir, &q.service, q.lines) {
        Ok(output) => Ok(Json(serde_json::json!({ "logs": output }))),
        Err(e) => {
            eprintln!("logs error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
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
    let compose_dir = state.compose_dir.clone();
    let service = q.service.clone();

    tokio::spawn(async move {
        if let Err(e) = logs::stream_logs(&compose_dir, &service, tx).await {
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

async fn restart(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ServiceQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;
    match actions::restart_service(&state.compose_dir, &q.service) {
        Ok(r) => Ok(Json(serde_json::to_value(r).unwrap())),
        Err(e) => {
            eprintln!("restart error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn stop(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ServiceQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;
    match actions::stop_service(&state.compose_dir, &q.service) {
        Ok(r) => Ok(Json(serde_json::to_value(r).unwrap())),
        Err(e) => {
            eprintln!("stop error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn start(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ServiceQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;
    match actions::start_service(&state.compose_dir, &q.service) {
        Ok(r) => Ok(Json(serde_json::to_value(r).unwrap())),
        Err(e) => {
            eprintln!("start error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn deploy(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    check_auth(&state, &headers)?;
    match actions::deploy(&state.compose_dir) {
        Ok(r) => Ok(Json(serde_json::to_value(r).unwrap())),
        Err(e) => {
            eprintln!("deploy error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
