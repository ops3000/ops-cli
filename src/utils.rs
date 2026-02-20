// src/utils.rs

use anyhow::{anyhow, Result};

/// Target type that supports both Node IDs and App targets
#[derive(Debug, Clone)]
pub enum Target {
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

impl Target {
    /// Get the path if any
    pub fn path(&self) -> Option<&str> {
        match self {
            Target::NodeId { path, .. } => path.as_deref(),
            Target::AppTarget { path, .. } => path.as_deref(),
        }
    }

    /// Get the domain for this target
    pub fn domain(&self) -> String {
        match self {
            Target::NodeId { id, .. } => format!("{}.node.ops.autos", id),
            Target::AppTarget { app, project, .. } => format!("{}.{}.ops.autos", app, project),
        }
    }

    /// Check if this is a node ID target
    pub fn is_node_id(&self) -> bool {
        matches!(self, Target::NodeId { .. })
    }
}

/// Parse a target string into Target
/// Supports:
/// - "12345" → NodeId
/// - "12345:/path" → NodeId with path
/// - "api.RedQ" → AppTarget
/// - "api.RedQ:/path" → AppTarget with path
pub fn parse_target(target_str: &str) -> Result<Target> {
    // 1. Split off the path (after colon)
    let (server_part, path_part) = match target_str.split_once(':') {
        Some((s, p)) => (s, Some(p.to_string())),
        None => (target_str, None),
    };

    // 2. Check if it's a pure node ID (numeric)
    if let Ok(id) = server_part.parse::<u64>() {
        return Ok(Target::NodeId { id, path: path_part });
    }

    // 3. Parse as app.project format
    let parts: Vec<&str> = server_part.split('.').collect();
    if parts.len() != 2 {
        return Err(anyhow!(
            "Invalid target format. Expected 'app.project' (e.g., api.RedQ) or node ID (e.g., 12345)"
        ));
    }

    Ok(Target::AppTarget {
        app: parts[0].to_string(),
        project: parts[1].to_string(),
        path: path_part,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_target_node_id() {
        let result = parse_target("12345").unwrap();
        match result {
            Target::NodeId { id, path } => {
                assert_eq!(id, 12345);
                assert!(path.is_none());
            }
            _ => panic!("Expected NodeId"),
        }
    }

    #[test]
    fn test_parse_target_node_id_with_path() {
        let result = parse_target("12345:/root/").unwrap();
        match result {
            Target::NodeId { id, path } => {
                assert_eq!(id, 12345);
                assert_eq!(path.as_deref(), Some("/root/"));
            }
            _ => panic!("Expected NodeId"),
        }
    }

    #[test]
    fn test_parse_target_app_target() {
        let result = parse_target("api.RedQ").unwrap();
        match result {
            Target::AppTarget { app, project, path } => {
                assert_eq!(app, "api");
                assert_eq!(project, "RedQ");
                assert!(path.is_none());
            }
            _ => panic!("Expected AppTarget"),
        }
    }

    #[test]
    fn test_parse_target_app_target_with_path() {
        let result = parse_target("api.RedQ:/var/www").unwrap();
        match result {
            Target::AppTarget { app, project, path } => {
                assert_eq!(app, "api");
                assert_eq!(project, "RedQ");
                assert_eq!(path.as_deref(), Some("/var/www"));
            }
            _ => panic!("Expected AppTarget"),
        }
    }

    #[test]
    fn test_target_domain() {
        let node = Target::NodeId { id: 12345, path: None };
        assert_eq!(node.domain(), "12345.node.ops.autos");

        let app = Target::AppTarget {
            app: "api".to_string(),
            project: "RedQ".to_string(),
            path: None,
        };
        assert_eq!(app.domain(), "api.RedQ.ops.autos");
    }
}
