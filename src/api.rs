use reqwest::{Client, Response, StatusCode};
use anyhow::{anyhow, Context, Result};
use crate::types::{
    ErrorResponse, LoginResponse, CiKeyResponse, RegisterResponse, WhoamiResponse,
    ProjectResponse, ServerWhoamiResponse, NodeSetResponseV2, ProjectListResponse,
    SyncAppResponse, CreateDeploymentResponse, UpdateDeploymentResponse,
    OpsToml, RouteDef,
    // Node Group types
    NodeGroupListResponse, NodeGroupDetailResponse, CreateNodeGroupResponse,
    // Nodes V2 types
    NodeInitResponse, NodeV2, NodeV2ListResponse, PrimaryNodeResponse,
    BindNodeResponse, BindByNameResponse, MessageResponse,
};

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

// Multi-region support: set_node with optional region, zone, hostname, weight
pub async fn set_node(
    token: &str,
    project: &str,
    environment: &str,
    ssh_pub_key: &str,
    force_reset: bool,
    region: Option<&str>,
    zone: Option<&str>,
    hostname: Option<&str>,
    weight: Option<u8>,
) -> Result<NodeSetResponseV2> {
    let client = Client::new();
    let mut body = serde_json::json!({
        "project": project,
        "environment": environment,
        "ssh_pub_key": ssh_pub_key,
        "force_reset": force_reset
    });

    // Add optional multi-region fields
    if let Some(r) = region {
        body["region"] = serde_json::Value::String(r.to_string());
    }
    if let Some(z) = zone {
        body["zone"] = serde_json::Value::String(z.to_string());
    }
    if let Some(h) = hostname {
        body["hostname"] = serde_json::Value::String(h.to_string());
    }
    if let Some(w) = weight {
        body["weight"] = serde_json::Value::Number(w.into());
    }

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

// ===== App Sync API (for ops deploy) =====

/// Extract "owner/repo" from git URL like "git@github.com:owner/repo.git"
fn extract_github_repo(git_url: &str) -> Option<String> {
    // Handle git@github.com:owner/repo.git format
    if git_url.contains("github.com") {
        let url = git_url
            .replace("git@github.com:", "")
            .replace("https://github.com/", "")
            .replace(".git", "");
        if url.contains('/') {
            return Some(url);
        }
    }
    None
}

/// Sync app record to backend (PUT /apps/sync)
pub async fn sync_app(token: &str, config: &OpsToml) -> Result<SyncAppResponse> {
    let client = Client::new();

    // Extract github_repo from git config
    let github_repo = config.deploy.git.as_ref()
        .and_then(|g| extract_github_repo(&g.repo));

    // Convert routes to API format
    let routes: Vec<serde_json::Value> = config.routes.iter().map(|r| {
        serde_json::json!({
            "domain": r.domain,
            "port": r.port,
            "ssl": r.ssl
        })
    }).collect();

    // Convert config to JSON
    let config_json = serde_json::to_string(config).ok();

    let body = serde_json::json!({
        "target": config.target,
        "name": config.app,
        "deploy_path": config.deploy_path,
        "github_repo": github_repo,
        "github_branch": config.deploy.branch.clone().unwrap_or_else(|| "main".to_string()),
        "routes": routes,
        "config_json": config_json,
    });

    let res = client
        .put(format!("{}/apps/sync", BASE_URL))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;

    handle_response(res).await
}

/// Create deployment record (POST /apps/:id/deployments)
pub async fn create_deployment(token: &str, app_id: i64, trigger: &str) -> Result<CreateDeploymentResponse> {
    let client = Client::new();
    let body = serde_json::json!({
        "trigger": trigger
    });

    let res = client
        .post(format!("{}/apps/{}/deployments", BASE_URL, app_id))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;

    handle_response(res).await
}

/// Update deployment status (PATCH /apps/deployments/:id)
pub async fn update_deployment(token: &str, deployment_id: i64, status: &str, logs: Option<&str>) -> Result<UpdateDeploymentResponse> {
    let client = Client::new();
    let body = serde_json::json!({
        "status": status,
        "logs": logs
    });

    let res = client
        .patch(format!("{}/apps/deployments/{}", BASE_URL, deployment_id))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;

    handle_response(res).await
}

// ===== Node Group API =====

/// Create a node group (POST /node-groups)
pub async fn create_node_group(
    token: &str,
    project: &str,
    environment: &str,
    name: Option<&str>,
    lb_strategy: &str,
) -> Result<CreateNodeGroupResponse> {
    let client = Client::new();
    let mut body = serde_json::json!({
        "project": project,
        "environment": environment,
        "lb_strategy": lb_strategy
    });

    if let Some(n) = name {
        body["name"] = serde_json::Value::String(n.to_string());
    }

    let res = client
        .post(format!("{}/node-groups", BASE_URL))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;

    handle_response(res).await
}

/// List node groups (GET /node-groups)
pub async fn list_node_groups(token: &str, project: Option<&str>) -> Result<NodeGroupListResponse> {
    let client = Client::new();
    let mut url = format!("{}/node-groups", BASE_URL);

    if let Some(p) = project {
        url = format!("{}?project={}", url, p);
    }

    let res = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await?;

    handle_response(res).await
}

/// Get node group details (GET /node-groups/:id)
pub async fn get_node_group(token: &str, id: i64) -> Result<NodeGroupDetailResponse> {
    let client = Client::new();
    let res = client
        .get(format!("{}/node-groups/{}", BASE_URL, id))
        .bearer_auth(token)
        .send()
        .await?;

    handle_response(res).await
}

/// Get nodes in environment (GET /nodes/:project/:environment)
#[derive(serde::Deserialize, Debug)]
pub struct NodesInEnvResponse {
    pub node_group: NodeGroupSummary,
    pub nodes: Vec<crate::types::NodeInGroup>,
}

#[derive(serde::Deserialize, Debug)]
pub struct NodeGroupSummary {
    pub id: i64,
    pub name: String,
    pub lb_strategy: String,
}

pub async fn get_nodes_in_env(token: &str, project: &str, environment: &str) -> Result<NodesInEnvResponse> {
    let client = Client::new();
    let res = client
        .get(format!("{}/nodes/{}/{}", BASE_URL, project, environment))
        .bearer_auth(token)
        .send()
        .await?;

    handle_response(res).await
}

// ===== Nodes V2 API (Global Nodes) =====

/// Initialize a new node (POST /nodes-v2/init)
pub async fn init_node(
    token: &str,
    ssh_pub_key: &str,
    region: Option<&str>,
    allowed_projects: Option<Vec<String>>,
    allowed_apps: Option<Vec<String>>,
    port: Option<u16>,
    hostname: Option<&str>,
) -> Result<NodeInitResponse> {
    let client = Client::new();
    let mut body = serde_json::json!({
        "ssh_pub_key": ssh_pub_key
    });

    if let Some(r) = region {
        body["region"] = serde_json::Value::String(r.to_string());
    }
    if let Some(p) = allowed_projects {
        body["allowed_projects"] = serde_json::json!(p);
    }
    if let Some(a) = allowed_apps {
        body["allowed_apps"] = serde_json::json!(a);
    }
    if let Some(port) = port {
        body["port"] = serde_json::Value::Number(port.into());
    }
    if let Some(h) = hostname {
        body["hostname"] = serde_json::Value::String(h.to_string());
    }

    let res = client
        .post(format!("{}/nodes-v2/init", BASE_URL))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;

    handle_response(res).await
}

/// Re-initialize an existing node (POST /nodes-v2/reinit)
/// Used to get serve token for daemon setup on a server that's already registered
pub async fn reinit_node(
    token: &str,
    ssh_pub_key: &str,
    region: Option<&str>,
    allowed_projects: Option<Vec<String>>,
    allowed_apps: Option<Vec<String>>,
    port: Option<u16>,
    hostname: Option<&str>,
) -> Result<NodeInitResponse> {
    let client = Client::new();
    let mut body = serde_json::json!({
        "ssh_pub_key": ssh_pub_key
    });

    if let Some(r) = region {
        body["region"] = serde_json::Value::String(r.to_string());
    }
    if let Some(p) = allowed_projects {
        body["allowed_projects"] = serde_json::json!(p);
    }
    if let Some(a) = allowed_apps {
        body["allowed_apps"] = serde_json::json!(a);
    }
    if let Some(port) = port {
        body["port"] = serde_json::Value::Number(port.into());
    }
    if let Some(h) = hostname {
        body["hostname"] = serde_json::Value::String(h.to_string());
    }

    let res = client
        .post(format!("{}/nodes-v2/reinit", BASE_URL))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;

    handle_response(res).await
}

/// List user's nodes (GET /nodes-v2)
pub async fn list_nodes_v2(token: &str) -> Result<NodeV2ListResponse> {
    let client = Client::new();
    let res = client
        .get(format!("{}/nodes-v2", BASE_URL))
        .bearer_auth(token)
        .send()
        .await?;

    handle_response(res).await
}

/// Get node by ID (GET /nodes-v2/:id)
pub async fn get_node_v2(token: &str, node_id: u64) -> Result<NodeV2> {
    let client = Client::new();
    let res = client
        .get(format!("{}/nodes-v2/{}", BASE_URL, node_id))
        .bearer_auth(token)
        .send()
        .await?;

    handle_response(res).await
}

/// Delete node (DELETE /nodes-v2/:id)
pub async fn delete_node_v2(token: &str, node_id: u64) -> Result<MessageResponse> {
    let client = Client::new();
    let res = client
        .delete(format!("{}/nodes-v2/{}", BASE_URL, node_id))
        .bearer_auth(token)
        .send()
        .await?;

    handle_response(res).await
}

/// Get CI key for node (GET /nodes-v2/:id/ci-key)
pub async fn get_node_ci_key(token: &str, node_id: u64) -> Result<CiKeyResponse> {
    let client = Client::new();
    let res = client
        .get(format!("{}/nodes-v2/{}/ci-key", BASE_URL, node_id))
        .bearer_auth(token)
        .send()
        .await?;

    handle_response(res).await
}

/// Get primary node for app (GET /apps/:project/:app/primary-node)
pub async fn get_app_primary_node(token: &str, project: &str, app: &str) -> Result<PrimaryNodeResponse> {
    let client = Client::new();
    let res = client
        .get(format!("{}/apps/{}/{}/primary-node", BASE_URL, project, app))
        .bearer_auth(token)
        .send()
        .await?;

    handle_response(res).await
}

/// Get CI key for app (GET /apps/:project/:app/ci-key)
pub async fn get_app_ci_key(token: &str, project: &str, app: &str) -> Result<CiKeyResponse> {
    let client = Client::new();
    let res = client
        .get(format!("{}/apps/{}/{}/ci-key", BASE_URL, project, app))
        .bearer_auth(token)
        .send()
        .await?;

    handle_response(res).await
}

/// Bind node to app (POST /apps/:id/bind)
pub async fn bind_app_node(
    token: &str,
    app_id: i64,
    node_id: u64,
    is_primary: bool,
    weight: Option<u8>,
) -> Result<BindNodeResponse> {
    let client = Client::new();
    let mut body = serde_json::json!({
        "node_id": node_id,
        "is_primary": is_primary
    });

    if let Some(w) = weight {
        body["weight"] = serde_json::Value::Number(w.into());
    }

    let res = client
        .post(format!("{}/apps/{}/bind", BASE_URL, app_id))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;

    handle_response(res).await
}

/// Bind node to app by project/app name (POST /apps/bind-by-name)
/// Creates the app if it doesn't exist
pub async fn bind_node_by_name(
    token: &str,
    project: &str,
    app: &str,
    node_id: u64,
    is_primary: bool,
    weight: Option<u8>,
) -> Result<BindByNameResponse> {
    let client = Client::new();
    let mut body = serde_json::json!({
        "project": project,
        "app": app,
        "node_id": node_id,
        "is_primary": is_primary
    });

    if let Some(w) = weight {
        body["weight"] = serde_json::Value::Number(w.into());
    }

    let res = client
        .post(format!("{}/apps/bind-by-name", BASE_URL))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;

    handle_response(res).await
}