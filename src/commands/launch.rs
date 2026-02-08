use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

/// 项目扫描结果
struct ProjectScan {
    dir_name: String,
    compose_files: Vec<String>,
    has_dockerfile: bool,
    has_git: bool,
    git_remote: Option<String>,
    language: Option<String>,
    has_env_file: bool,
    sync_candidates: Vec<String>,
}

/// 扫描当前目录，检测项目类型和配置文件
fn scan_project() -> ProjectScan {
    let dir_name = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "app".to_string());

    let compose_files: Vec<String> = [
        "docker-compose.yml",
        "docker-compose.yaml",
        "docker-compose.prod.yml",
        "docker-compose.prod.yaml",
        "docker-compose.base.yml",
        "docker-compose.base.yaml",
        "docker-compose.override.yml",
        "docker-compose.override.yaml",
    ]
    .iter()
    .filter(|f| Path::new(f).exists())
    .map(|f| f.to_string())
    .collect();

    let has_dockerfile = Path::new("Dockerfile").exists();
    let has_git = Path::new(".git").exists();
    let git_remote = if has_git {
        Command::new("git")
            .args(["remote", "get-url", "origin"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
    } else {
        None
    };

    let language = if Path::new("Cargo.toml").exists() {
        Some("rust")
    } else if Path::new("package.json").exists() {
        Some("node")
    } else if Path::new("go.mod").exists() {
        Some("go")
    } else if Path::new("requirements.txt").exists() || Path::new("pyproject.toml").exists() {
        Some("python")
    } else {
        None
    }
    .map(String::from);

    let has_env_file = Path::new(".env").exists();

    let sync_candidates: Vec<String> = [
        "Config.toml",
        "config.yaml",
        "config.yml",
        "config.json",
        "nginx.conf",
    ]
    .iter()
    .filter(|f| Path::new(f).exists())
    .map(|f| f.to_string())
    .collect();

    ProjectScan {
        dir_name,
        compose_files,
        has_dockerfile,
        has_git,
        git_remote,
        language,
        has_env_file,
        sync_candidates,
    }
}

/// 推断默认 deploy source
fn suggest_source(scan: &ProjectScan) -> &str {
    if !scan.compose_files.is_empty() && !scan.has_dockerfile {
        "image"
    } else if scan.has_dockerfile {
        "push"
    } else if scan.git_remote.is_some() {
        "git"
    } else {
        "push"
    }
}

/// 带默认值的交互提示。--yes 模式直接用默认值
fn prompt_with_default(prompt: &str, default: &str, yes: bool) -> Result<String> {
    if yes {
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

/// 带默认值的交互提示（允许空值）。--yes 模式返回空
fn prompt_optional(prompt: &str, yes: bool) -> Result<String> {
    if yes {
        return Ok(String::new());
    }
    o_print!("  {} ", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

/// 确认提示，默认 Yes。--yes 模式直接返回 true
fn prompt_confirm_yes(prompt: &str, yes: bool) -> Result<bool> {
    if yes {
        return Ok(true);
    }
    o_print!("  {} [Y/n]: ", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();
    Ok(input.is_empty() || input == "y" || input == "yes")
}

/// 确认提示，默认 No
fn prompt_confirm_no(prompt: &str, yes: bool) -> Result<bool> {
    if yes {
        return Ok(false);
    }
    o_print!("  {} [y/N]: ", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();
    Ok(input == "y" || input == "yes")
}

/// 收集的配置
struct LaunchConfig {
    app_name: String,
    target: String,
    deploy_path: String,
    source: String,
    git_repo: Option<String>,
    git_branch: Option<String>,
    compose_files: Vec<String>,
    use_registry: bool,
    env_files: Vec<(String, String)>,
    sync_files: Vec<(String, String)>,
    health_url: Option<String>,
}

/// 生成 ops.toml 内容
fn generate_toml(cfg: &LaunchConfig) -> String {
    let mut out = String::new();

    // Header
    out.push_str(&format!("app = \"{}\"\n", cfg.app_name));
    if !cfg.target.is_empty() {
        out.push_str(&format!("target = \"{}\"\n", cfg.target));
    }
    out.push_str(&format!("deploy_path = \"{}\"\n", cfg.deploy_path));

    // [deploy]
    out.push_str(&format!("\n[deploy]\nsource = \"{}\"\n", cfg.source));

    if cfg.source == "git" {
        if let Some(ref repo) = cfg.git_repo {
            let branch = cfg.git_branch.as_deref().unwrap_or("main");
            out.push_str(&format!("branch = \"{}\"\n", branch));
            out.push_str(&format!("\n[deploy.git]\nrepo = \"{}\"\n", repo));
            out.push_str("# ssh_key = \"~/.ssh/deploy_key\"\n");
        }
    }

    if cfg.source == "image" && !cfg.compose_files.is_empty() {
        let files: Vec<String> = cfg.compose_files.iter().map(|f| format!("\"{}\"", f)).collect();
        out.push_str(&format!("compose_files = [{}]\n", files.join(", ")));
    }

    // Registry
    if cfg.source == "image" && cfg.use_registry {
        out.push_str("\n[deploy.registry]\nurl = \"ghcr.io\"\ntoken = \"$GHCR_PAT\"\n");
    } else if cfg.source == "image" {
        out.push_str("\n# [deploy.registry]\n# url = \"ghcr.io\"\n# token = \"$GHCR_PAT\"\n");
    }

    // compose_files for non-image source (if detected)
    if cfg.source != "image" && !cfg.compose_files.is_empty() {
        let files: Vec<String> = cfg.compose_files.iter().map(|f| format!("\"{}\"", f)).collect();
        out.push_str(&format!("compose_files = [{}]\n", files.join(", ")));
    }

    // env_files
    for (local, remote) in &cfg.env_files {
        out.push_str(&format!(
            "\n[[env_files]]\nlocal = \"{}\"\nremote = \"{}\"\n",
            local, remote
        ));
    }

    // sync
    for (local, remote) in &cfg.sync_files {
        out.push_str(&format!(
            "\n[[sync]]\nlocal = \"{}\"\nremote = \"{}\"\n",
            local, remote
        ));
    }

    // Sync compose files for image mode
    if cfg.source == "image" {
        for f in &cfg.compose_files {
            // Don't duplicate if already in sync_files
            if !cfg.sync_files.iter().any(|(l, _)| l == f) {
                out.push_str(&format!(
                    "\n[[sync]]\nlocal = \"./{}\"\nremote = \"{}\"\n",
                    f, f
                ));
            }
        }
    }

    // healthchecks
    if let Some(ref url) = cfg.health_url {
        out.push_str(&format!(
            "\n[[healthchecks]]\nname = \"Health\"\nurl = \"{}\"\n",
            url
        ));
    } else {
        out.push_str("\n# [[healthchecks]]\n# name = \"Health\"\n# url = \"http://localhost:8000/health\"\n");
    }

    out
}

/// ops launch 主入口
pub async fn handle_launch(output: String, yes: bool) -> Result<()> {
    o_step!();
    o_step!("{}", "OPS Launch".cyan().bold());
    o_step!("{}", "══════════".cyan());
    o_step!();

    // 1. 扫描项目
    o_step!("{}", "Scanning project...".cyan());
    let scan = scan_project();

    // 打印检测结果
    if !scan.compose_files.is_empty() {
        o_detail!(
            "  {} {}",
            "✔ Detected:".green(),
            scan.compose_files.join(", ")
        );
    }
    if scan.has_dockerfile {
        o_detail!("  {} Dockerfile", "✔ Detected:".green());
    }
    if scan.has_git {
        if let Some(ref remote) = scan.git_remote {
            o_detail!("  {} git remote: {}", "✔ Detected:".green(), remote);
        } else {
            o_detail!("  {} git repository (no remote)", "✔ Detected:".green());
        }
    }
    if scan.has_env_file {
        o_detail!("  {} .env", "✔ Detected:".green());
    }
    if let Some(ref lang) = scan.language {
        o_detail!("  {} Language: {}", "✔ Detected:".green(), lang);
    }
    if !scan.sync_candidates.is_empty() {
        o_detail!(
            "  {} Config files: {}",
            "✔ Detected:".green(),
            scan.sync_candidates.join(", ")
        );
    }
    o_detail!();

    // 2. 检查已有 ops.toml
    if Path::new(&output).exists() && !yes {
        if !prompt_confirm_no(&format!("{} already exists. Overwrite?", output), false)? {
            o_warn!("Aborted.");
            return Ok(());
        }
        o_detail!();
    }

    // 3. 交互提问
    let default_source = suggest_source(&scan);

    let app_name = prompt_with_default("App name", &scan.dir_name, yes)?;
    let source = prompt_with_default("Deploy source (git/push/image)", default_source, yes)?;
    let default_path = format!("/opt/{}", app_name);
    let deploy_path = prompt_with_default("Deploy path", &default_path, yes)?;
    let target = prompt_optional(
        "Target (e.g. prod.myproject, enter to skip):",
        yes,
    )?;

    // Git config
    let (git_repo, git_branch) = if source == "git" {
        let default_repo = scan.git_remote.as_deref().unwrap_or("");
        let repo = prompt_with_default("Git repo URL", default_repo, yes)?;
        let branch = prompt_with_default("Git branch", "main", yes)?;
        (Some(repo), Some(branch))
    } else {
        (None, None)
    };

    // Compose files
    let compose_files = if !scan.compose_files.is_empty() {
        o_detail!();
        o_detail!("  Found docker-compose files:");
        for (i, f) in scan.compose_files.iter().enumerate() {
            o_detail!("    {}. {}", i + 1, f);
        }
        if prompt_confirm_yes("Use these compose files?", yes)? {
            scan.compose_files.clone()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Registry (image mode only)
    let use_registry = if source == "image" {
        o_detail!();
        prompt_confirm_no("Configure container registry (ghcr.io)?", yes)?
    } else {
        false
    };

    // Env files
    let env_files = if scan.has_env_file {
        if prompt_confirm_yes("Sync .env to remote?", yes)? {
            vec![(".env".to_string(), ".env".to_string())]
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Sync candidates
    let sync_files: Vec<(String, String)> = if !scan.sync_candidates.is_empty() {
        let mut files = Vec::new();
        for f in &scan.sync_candidates {
            if prompt_confirm_yes(&format!("Sync {} to remote?", f), yes)? {
                files.push((format!("./{}", f), f.clone()));
            }
        }
        files
    } else {
        Vec::new()
    };

    // Health check
    o_detail!();
    let health_url = prompt_optional(
        "Health check URL (enter to skip):",
        yes,
    )?;
    let health_url = if health_url.is_empty() {
        None
    } else {
        Some(health_url)
    };

    // 4. 生成 TOML
    let cfg = LaunchConfig {
        app_name,
        target,
        deploy_path,
        source,
        git_repo,
        git_branch,
        compose_files,
        use_registry,
        env_files,
        sync_files,
        health_url,
    };

    let content = generate_toml(&cfg);

    // 5. 写文件
    fs::write(&output, &content)
        .with_context(|| format!("Failed to write {}", output))?;

    o_result!();
    o_result!("{} Generated {}", "✔".green(), output.cyan());
    o_detail!();
    o_detail!("{}", "Next steps:".cyan().bold());
    o_detail!("  {}          # deploy all services", "ops deploy".cyan());
    o_detail!(
        "  {}  # deploy a specific app group",
        "ops deploy --app <name>".cyan()
    );
    o_detail!(
        "  {}          # check deployment status",
        "ops status".cyan()
    );

    Ok(())
}
