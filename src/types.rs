use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(Deserialize, Debug)]
pub struct NodeSetResponse {
    pub message: String,
    pub ci_ssh_public_key: String,
}

#[derive(Deserialize, Debug)]
pub struct CiKeyResponse {
    pub private_key: String,
}

#[derive(Deserialize, Debug)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Deserialize, Debug)]
pub struct RegisterResponse {
    pub message: String,
}

#[derive(Deserialize, Debug)]
pub struct WhoamiResponse {
    #[serde(rename = "userId")]
    pub user_id: i64,
    pub username: String,
    pub token_expires_at: String,
}

#[derive(Deserialize, Debug)]
pub struct ProjectResponse {
    pub message: String,
}

#[derive(Deserialize, Debug)]
pub struct ServerWhoamiResponse {
    pub ip_address: String,
    pub status: String,
    pub domain: Option<String>,
    pub project: Option<String>,
    pub owner: Option<String>,
    pub permission: Option<String>,
    pub message: Option<String>,
}

// --- 新增：项目列表相关的结构体 ---

#[derive(Deserialize, Debug)]
pub struct NodeItem {
    pub environment: String,
    pub ip_address: String,
    pub domain: String,
}

#[derive(Deserialize, Debug)]
pub struct ProjectItem {
    pub name: String,
    pub nodes: Vec<NodeItem>,
}

#[derive(Deserialize, Debug)]
pub struct ProjectListResponse {
    pub projects: Vec<ProjectItem>,
}

// ===== ops.toml 配置结构 =====

fn default_source() -> String { "git".into() }

#[derive(Deserialize, Serialize, Debug)]
pub struct OpsToml {
    pub app: String,
    pub target: String,
    pub deploy_path: String,
    pub deploy: DeployConfig,
    #[serde(default)]
    pub env_files: Vec<EnvFileMapping>,
    #[serde(default)]
    pub sync: Vec<SyncMapping>,
    #[serde(default)]
    pub routes: Vec<RouteDef>,
    #[serde(default)]
    pub healthchecks: Vec<HealthCheck>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct DeployConfig {
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub git: Option<GitConfig>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct GitConfig {
    pub repo: String,
    pub ssh_key: Option<String>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct EnvFileMapping {
    pub local: String,
    pub remote: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct SyncMapping {
    pub local: String,
    pub remote: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct RouteDef {
    pub domain: String,
    pub port: u16,
    #[serde(default)]
    pub ssl: bool,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct HealthCheck {
    pub name: String,
    pub url: String,
}

// ===== App Sync API 结构 =====

#[derive(Deserialize, Debug)]
pub struct SyncAppResponse {
    pub app_id: i64,
    pub created: bool,
    pub message: String,
}

#[derive(Deserialize, Debug)]
pub struct CreateDeploymentResponse {
    pub id: i64,
    pub status: String,
}

#[derive(Deserialize, Debug)]
pub struct UpdateDeploymentResponse {
    pub success: bool,
}

// ===== Node Group API 结构 =====

#[derive(Deserialize, Debug)]
pub struct NodeGroup {
    pub id: i64,
    pub environment: String,
    pub name: String,
    pub lb_strategy: String,
    pub project_name: Option<String>,
    pub node_count: Option<i64>,
    pub healthy_count: Option<i64>,
}

#[derive(Deserialize, Debug)]
pub struct NodeInGroup {
    pub id: i64,
    pub hostname: Option<String>,
    pub ip_address: String,
    pub domain: String,
    pub region: Option<String>,
    pub zone: Option<String>,
    pub weight: i64,
    pub status: String,
    pub last_health_check: Option<String>,
    pub has_serve_token: Option<i64>,
}

#[derive(Deserialize, Debug)]
pub struct NodeGroupListResponse {
    pub node_groups: Vec<NodeGroup>,
}

#[derive(Deserialize, Debug)]
pub struct NodeGroupDetailResponse {
    pub id: i64,
    pub environment: String,
    pub name: String,
    pub lb_strategy: String,
    pub project_name: String,
    pub nodes: Vec<NodeInGroup>,
    pub health_config: Option<HealthCheckConfig>,
}

#[derive(Deserialize, Debug)]
pub struct HealthCheckConfig {
    pub check_type: String,
    pub endpoint: String,
    pub interval_seconds: i64,
    pub timeout_seconds: i64,
    pub unhealthy_threshold: i64,
    pub healthy_threshold: i64,
}

#[derive(Deserialize, Debug)]
pub struct CreateNodeGroupResponse {
    pub message: String,
    pub node_group: NodeGroup,
}

#[derive(Deserialize, Debug)]
pub struct NodeSetResponseV2 {
    pub message: String,
    pub domain: String,
    pub node_id: i64,
    pub node_group_id: i64,
    pub ci_ssh_public_key: String,
    pub region: Option<String>,
}

// ===== Nodes V2 API (Global Nodes) =====

#[derive(Deserialize, Debug)]
pub struct NodeInitResponse {
    pub message: String,
    pub node_id: i64,
    pub domain: String,
    pub ip_address: String,
    pub serve_token: String,
    pub serve_port: u16,
    pub ci_ssh_public_key: String,
    pub region: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct NodeV2 {
    pub id: i64,
    pub ip_address: String,
    pub hostname: Option<String>,
    pub domain: String,
    pub region: Option<String>,
    pub zone: Option<String>,
    pub serve_port: u16,
    pub allowed_projects: Option<Vec<String>>,
    pub allowed_apps: Option<Vec<String>>,
    pub status: String,
    pub last_health_check: Option<String>,
    pub has_serve_token: i64,
    pub created_at: String,
    pub bound_apps: Option<Vec<BoundApp>>,
}

#[derive(Deserialize, Debug)]
pub struct BoundApp {
    pub id: i64,
    pub name: String,
    pub project_name: String,
    pub is_primary: Option<i64>,
}

#[derive(Deserialize, Debug)]
pub struct NodeV2ListResponse {
    pub nodes: Vec<NodeV2>,
}

#[derive(Deserialize, Debug)]
pub struct PrimaryNodeResponse {
    pub node_id: i64,
    pub domain: String,
    pub ip_address: String,
    pub hostname: Option<String>,
    pub region: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct BindNodeResponse {
    pub message: String,
    pub mode: String,
    pub primary_node_id: Option<i64>,
    pub node_group_id: Option<i64>,
    pub total_nodes: Option<i64>,
    pub is_primary: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct BindByNameResponse {
    pub message: String,
    pub app_id: i64,
    pub mode: String,
    pub primary_node_id: Option<i64>,
    pub node_group_id: Option<i64>,
    pub total_nodes: Option<i64>,
    pub domain: String,
}

#[derive(Deserialize, Debug)]
pub struct UnbindNodeResponse {
    pub message: String,
    pub mode: String,
    pub remaining_nodes: Option<i64>,
}

#[derive(Deserialize, Debug)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Deserialize, Debug)]
pub struct RegenerateTokenResponse {
    pub message: String,
    pub serve_token: String,
}