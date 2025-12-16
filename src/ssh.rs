// src/ssh.rs

use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::io::Write;

// --- CHANGE HERE ---
// Add `pub` to make this function visible to other modules.
pub fn get_default_pubkey() -> Result<String> {
    let pubkey_path = dirs::home_dir()
        .context("Could not find home directory")?
        .join(".ssh")
        .join("id_rsa.pub");
        
    let content = fs::read_to_string(&pubkey_path)
        .with_context(|| format!("Failed to read SSH public key from {:?}", pubkey_path))?;
        
    Ok(content.trim().to_string())
}

// --- CHANGE HERE ---
// Add `pub` to make this function visible to other modules.
pub fn add_to_authorized_keys(pubkey: &str) -> Result<()> {
    let authorized_keys_path = dirs::home_dir()
        .context("Could not find home directory")?
        .join(".ssh")
        .join("authorized_keys");

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&authorized_keys_path)
        .with_context(|| format!("Failed to open authorized_keys file at {:?}", authorized_keys_path))?;

    // Prepending a newline ensures our entry doesn't get appended to the last line
    // if the file doesn't end with a newline character.
    writeln!(file, "\n# Added by ops.autos CLI for CI/CD")?;
    writeln!(file, "{}", pubkey)?;

    Ok(())
}