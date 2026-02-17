use anyhow::Result;
use colored::Colorize;
use std::io::{self, Write};

/// Confirm prompt with default Yes. Non-interactive returns true.
pub fn confirm_yes(msg: &str, interactive: bool) -> Result<bool> {
    if !interactive {
        return Ok(true);
    }
    o_print!("  {} [Y/n]: ", msg);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();
    Ok(input.is_empty() || input == "y" || input == "yes")
}

/// Confirm prompt with default No. Non-interactive returns false.
pub fn confirm_no(msg: &str, interactive: bool) -> Result<bool> {
    if !interactive {
        return Ok(false);
    }
    o_print!("  {} [y/N]: ", msg);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();
    Ok(input == "y" || input == "yes")
}

/// Text input with a default value. Non-interactive returns the default.
pub fn input_with_default(prompt: &str, default: &str, interactive: bool) -> Result<String> {
    if !interactive {
        return Ok(default.to_string());
    }
    o_print!("  {} [{}]: ", prompt, default.dimmed());
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    if input.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(input.to_string())
    }
}

/// Optional text input. Non-interactive returns empty string.
pub fn input_optional(prompt: &str, interactive: bool) -> Result<String> {
    if !interactive {
        return Ok(String::new());
    }
    o_print!("  {} ", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

/// Menu selection. Non-interactive returns default_index.
pub fn select(prompt: &str, options: &[&str], default_index: usize, interactive: bool) -> Result<usize> {
    if !interactive {
        return Ok(default_index);
    }
    for (i, opt) in options.iter().enumerate() {
        o_detail!("   {} {}", format!("{})", i + 1).cyan(), opt);
    }
    o_print!("\n   {} [{}]: ", prompt, default_index + 1);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice = input.trim();
    if choice.is_empty() {
        return Ok(default_index);
    }
    match choice.parse::<usize>() {
        Ok(n) if n >= 1 && n <= options.len() => Ok(n - 1),
        _ => Ok(default_index),
    }
}
