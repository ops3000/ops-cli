use serde::Deserialize;

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