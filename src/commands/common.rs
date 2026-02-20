use anyhow::{Context, Result};

/// Resolve "$ENV_VAR" → read environment variable value
pub fn resolve_env_value(val: &str) -> Result<String> {
    if val.starts_with('$') {
        std::env::var(&val[1..])
            .with_context(|| format!("Environment variable {} not set", val))
    } else {
        Ok(val.to_string())
    }
}
