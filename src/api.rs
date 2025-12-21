use reqwest::{Client, Response, StatusCode};
use anyhow::{anyhow, Context, Result};
use crate::types::{ErrorResponse, LoginResponse, CiKeyResponse, RegisterResponse, WhoamiResponse, ProjectResponse, ServerWhoamiResponse, NodeSetResponse, ProjectListResponse};

const BASE_URL: &str = "https://api.ops.autos";

async fn handle_response<T: serde::de::DeserializeOwned>(res: Response) -> Result<T> {
    let status = res.status();
    if status.is_success() {
        res.json::<T>().await.context("Failed to parse success response")
    } else {
        let error_text = res.text().await.unwrap_or_else(|_| format!("HTTP Error: {}", status));
        let error_res: Result<ErrorResponse, _> = serde_json::from_str(&error_text);
        match error_res {
            Ok(parsed_err) => Err(anyhow!(parsed_err.error)),
            Err(_) => Err(anyhow!(error_text)),
        }
    }
}

pub async fn register(username: &str, password: &str) -> Result<RegisterResponse> {
    let client = Client::new();
    let body = serde_json::json!({ "username": username, "password": password });
    let res = client.post(format!("{}/auth/register", BASE_URL)).json(&body).send().await?;
    handle_response(res).await
}

pub async fn login(username: &str, password: &str) -> Result<LoginResponse> {
    let client = Client::new();
    let body = serde_json::json!({ "username": username, "password": password });
    let res = client.post(format!("{}/auth/login", BASE_URL)).json(&body).send().await?;
    handle_response(res).await
}

pub async fn whoami(token: &str) -> Result<WhoamiResponse> {
    let client = Client::new();
    let res = client
        .get(format!("{}/me", BASE_URL))
        .bearer_auth(token)
        .send()
        .await?;
    handle_response(res).await
}

pub async fn create_project(token: &str, name: &str) -> Result<ProjectResponse> {
    let client = Client::new();
    let body = serde_json::json!({ "name": name });
    let res = client.post(format!("{}/projects", BASE_URL))
        .bearer_auth(token).json(&body).send().await?;
    handle_response(res).await
}

// 支持 ops project list
pub async fn list_projects(token: &str, name_filter: Option<&str>) -> Result<ProjectListResponse> {
    let client = Client::new();
    let mut url = format!("{}/projects", BASE_URL);
    if let Some(name) = name_filter {
        url = format!("{}?name={}", url, name);
    }
    let res = client.get(&url).bearer_auth(token).send().await?;
    handle_response(res).await
}

pub async fn server_whoami(token: Option<&str>) -> Result<ServerWhoamiResponse> {
    let client = Client::new();
    let mut request_builder = client.get(format!("{}/server/whoami", BASE_URL));
    if let Some(t) = token {
        request_builder = request_builder.bearer_auth(t);
    }
    let res = request_builder.send().await?;
    handle_response(res).await
}

// --- 修复重点：参数增加 force_reset ---
pub async fn set_node(token: &str, project: &str, environment: &str, ssh_pub_key: &str, force_reset: bool) -> Result<NodeSetResponse> {
    let client = Client::new();
    let body = serde_json::json!({ 
        "project": project, 
        "environment": environment, 
        "ssh_pub_key": ssh_pub_key,
        "force_reset": force_reset 
    });
    let res = client.post(format!("{}/nodes/set", BASE_URL))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;
    handle_response(res).await
}

pub async fn get_ci_private_key(token: &str, project: &str, environment: &str) -> Result<CiKeyResponse> {
    let client = Client::new();
    let url = format!("{}/nodes/{}/{}/ci-private-key", BASE_URL, project, environment);
    let res = client.get(&url).bearer_auth(token).send().await?;
    handle_response(res).await
}