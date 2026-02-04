// src/utils.rs

use anyhow::{anyhow, Result};

/// Legacy target format for backward compatibility
pub struct Target {
    pub project: String,
    pub environment: String,
    pub path: Option<String>,
}

/// New target type that supports both Node IDs and App targets
#[derive(Debug, Clone)]
pub enum TargetType {
    /// Direct node ID (e.g., "12345" or "12345:/path")
    NodeId {
        id: u64,
        path: Option<String>,
    },
    /// App target (e.g., "api.RedQ" or "api.RedQ:/path")
    AppTarget {
        app: String,
        project: String,
        path: Option<String>,
    },
}

impl TargetType {
    /// Get the path if any
    pub fn path(&self) -> Option<&str> {
        match self {
            TargetType::NodeId { path, .. } => path.as_deref(),
            TargetType::AppTarget { path, .. } => path.as_deref(),
        }
    }

    /// Get the domain for this target
    pub fn domain(&self) -> String {
        match self {
            TargetType::NodeId { id, .. } => format!("{}.node.ops.autos", id),
            TargetType::AppTarget { app, project, .. } => format!("{}.{}.ops.autos", app, project),
        }
    }

    /// Check if this is a node ID target
    pub fn is_node_id(&self) -> bool {
        matches!(self, TargetType::NodeId { .. })
    }
}

/// Parse a target string into TargetType
/// Supports:
/// - "12345" → NodeId
/// - "12345:/path" → NodeId with path
/// - "api.RedQ" → AppTarget
/// - "api.RedQ:/path" → AppTarget with path
pub fn parse_target_v2(target_str: &str) -> Result<TargetType> {
    // 1. Split off the path (after colon)
    let (server_part, path_part) = match target_str.split_once(':') {
        Some((s, p)) => (s, Some(p.to_string())),
        None => (target_str, None),
    };

    // 2. Check if it's a pure node ID (numeric)
    if let Ok(id) = server_part.parse::<u64>() {
        return Ok(TargetType::NodeId { id, path: path_part });
    }

    // 3. Parse as app.project format
    let parts: Vec<&str> = server_part.split('.').collect();
    if parts.len() != 2 {
        return Err(anyhow!(
            "Invalid target format. Expected 'app.project' (e.g., api.RedQ) or node ID (e.g., 12345)"
        ));
    }

    Ok(TargetType::AppTarget {
        app: parts[0].to_string(),
        project: parts[1].to_string(),
        path: path_part,
    })
}

/// Legacy: Parse "environment.project" or "environment.project:/var/www"
/// Kept for backward compatibility
pub fn parse_target(target_str: &str) -> Result<Target> {
    // 1. Split off the remote path (if colon exists)
    let (server_part, path_part) = match target_str.split_once(':') {
        Some((s, p)) => (s, Some(p.to_string())),
        None => (target_str, None),
    };

    // 2. Parse environment.project
    let parts: Vec<&str> = server_part.split('.').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid target format. Expected 'environment.project' (e.g., main_server.jug0)"));
    }

    Ok(Target {
        environment: parts[0].to_string(),
        project: parts[1].to_string(),
        path: path_part,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_target_v2_node_id() {
        let result = parse_target_v2("12345").unwrap();
        match result {
            TargetType::NodeId { id, path } => {
                assert_eq!(id, 12345);
                assert!(path.is_none());
            }
            _ => panic!("Expected NodeId"),
        }
    }

    #[test]
    fn test_parse_target_v2_node_id_with_path() {
        let result = parse_target_v2("12345:/root/").unwrap();
        match result {
            TargetType::NodeId { id, path } => {
                assert_eq!(id, 12345);
                assert_eq!(path.as_deref(), Some("/root/"));
            }
            _ => panic!("Expected NodeId"),
        }
    }

    #[test]
    fn test_parse_target_v2_app_target() {
        let result = parse_target_v2("api.RedQ").unwrap();
        match result {
            TargetType::AppTarget { app, project, path } => {
                assert_eq!(app, "api");
                assert_eq!(project, "RedQ");
                assert!(path.is_none());
            }
            _ => panic!("Expected AppTarget"),
        }
    }

    #[test]
    fn test_parse_target_v2_app_target_with_path() {
        let result = parse_target_v2("api.RedQ:/var/www").unwrap();
        match result {
            TargetType::AppTarget { app, project, path } => {
                assert_eq!(app, "api");
                assert_eq!(project, "RedQ");
                assert_eq!(path.as_deref(), Some("/var/www"));
            }
            _ => panic!("Expected AppTarget"),
        }
    }

    #[test]
    fn test_target_type_domain() {
        let node = TargetType::NodeId { id: 12345, path: None };
        assert_eq!(node.domain(), "12345.node.ops.autos");

        let app = TargetType::AppTarget {
            app: "api".to_string(),
            project: "RedQ".to_string(),
            path: None,
        };
        assert_eq!(app.domain(), "api.RedQ.ops.autos");
    }
}