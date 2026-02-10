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

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct OpsToml {
    pub app: Option<String>,                    // 旧模式
    pub project: Option<String>,                // 新：项目模式
    pub target: Option<String>,                 // 旧模式必填，project 模式可选（自动解析）
    pub deploy_path: String,
    pub deploy: DeployConfig,
    #[serde(default)]
    pub apps: Vec<AppDef>,                      // 新：app 分组
    #[serde(default)]
    pub domains: Vec<String>,                   // legacy 模式自定义域名
    #[serde(default)]
    pub env_files: Vec<EnvFileMapping>,
    #[serde(default)]
    pub sync: Vec<SyncMapping>,
    #[serde(default)]
    pub routes: Vec<RouteDef>,
    #[serde(default)]
    pub healthchecks: Vec<HealthCheck>,
    pub build: Option<BuildConfig>,             // 远程构建配置
}


// ===== 远程构建配置 =====

fn default_build_source() -> String { "git".into() }
fn default_binary_arg() -> String { "SERVICE_BINARY".into() }

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BuildConfig {
    pub node: Option<u64>,                      // 构建节点 ID (可选，不指定则自动解析)
    pub path: String,                           // 远程构建目录
    pub command: String,                        // 构建命令
    #[serde(default = "default_build_source")]
    pub source: String,                         // "git" | "push"
    pub branch: Option<String>,                 // 默认分支
    pub git: Option<BuildGitConfig>,            // Git 配置
    pub image: Option<BuildImageConfig>,        // Docker 镜像打包
}


#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BuildGitConfig {
    pub repo: String,
    pub ssh_key: Option<String>,
    pub token: Option<String>,                  // HTTPS token, 支持 $ENV_VAR
}


#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BuildImageConfig {
    pub dockerfile: String,                     // e.g. "Dockerfile.prod"
    pub registry: String,                       // e.g. "ghcr.io"
    pub token: String,                          // e.g. "$GHCR_PAT"
    #[serde(default = "default_registry_username")]
    pub username: String,                       // e.g. "oauth2"
    pub prefix: String,                         // e.g. "ghcr.io/scheissedu/redq"
    #[serde(default = "default_binary_arg")]
    pub binary_arg: String,                     // Dockerfile ARG name
    pub services: Vec<String>,                  // 服务列表
}


#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DeployConfig {
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub git: Option<GitConfig>,
    #[serde(default)]
    pub compose_files: Option<Vec<String>>,
    #[serde(default)]
    pub registry: Option<RegistryConfig>,
}


#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AppDef {
    pub name: String,
    pub services: Vec<String>,
    #[serde(default)]
    pub domains: Vec<String>,
}


#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RegistryConfig {
    pub url: String,
    pub token: String,
    #[serde(default = "default_registry_username")]
    pub username: String,
}


fn default_registry_username() -> String { "oauth2".into() }

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GitConfig {
    pub repo: String,
    pub ssh_key: Option<String>,
}


#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct EnvFileMapping {
    pub local: String,
    pub remote: String,
}


#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SyncMapping {
    pub local: String,
    pub remote: String,
}


#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RouteDef {
    pub domain: String,
    pub port: u16,
    #[serde(default)]
    pub ssl: bool,
}


#[derive(Deserialize, Serialize, Debug, Clone)]
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


// ===== Custom Domains API =====

#[derive(Deserialize, Debug)]
pub struct AddDomainResponse {
    pub message: String,
    pub domain: String,
    pub cname_target: String,
    pub ssl_status: String,
    pub instructions: String,
    pub domain_connect_url: Option<String>,
}


#[derive(Deserialize, Debug)]
pub struct DomainItem {
    pub domain: String,
    pub status: String,
    pub created_at: String,
    pub cname_target: Option<String>,
}


#[derive(Deserialize, Debug)]
pub struct ListDomainsResponse {
    pub domains: Vec<DomainItem>,
    pub default_domain: String,
}


// ===== Deploy Targets API (multi-node deployment) =====

#[derive(Deserialize, Debug, Clone)]
pub struct DeployTarget {
    pub node_id: i64,
    pub domain: String,
    pub ip_address: String,
    pub hostname: Option<String>,
    pub region: Option<String>,
    pub zone: Option<String>,
    pub weight: i64,
    pub is_primary: bool,
    pub status: String,
}


#[derive(Deserialize, Debug)]
pub struct DeployTargetsResponse {
    pub mode: String,
    pub node_group_id: Option<i64>,
    pub lb_strategy: Option<String>,
    pub targets: Vec<DeployTarget>,
}

#[derive(Deserialize, Debug)]
pub struct CreateTunnelResponse {
    pub tunnel_id: i64,
    pub domain: String,
    pub node_ip: String,
}

