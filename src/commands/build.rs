use crate::commands::deploy::load_ops_toml;
use crate::commands::ssh;
use crate::types::{BuildConfig, OpsToml};
use crate::{api, config};
use anyhow::{Context, Result};
use colored::Colorize;
use std::time::Instant;

/// 解析 "$ENV_VAR" → 读环境变量值
fn resolve_env_value(val: &str) -> Result<String> {
    if val.starts_with('$') {
        std::env::var(&val[1..])
            .with_context(|| format!("Environment variable {} not set", val))
    } else {
        Ok(val.to_string())
    }
}

/// 解析构建节点，优先级：build.node → config.target → API 自动查询
async fn resolve_build_node(config: &OpsToml, build: &BuildConfig) -> Result<String> {
    // 1. 显式指定的 node ID
    if let Some(id) = build.node {
        return Ok(id.to_string());
    }
    // 2. ops.toml 顶层 target
    if let Some(ref t) = config.target {
        return Ok(t.clone());
    }
    // 3. 从 API 自动查询项目绑定的节点
    let project = config.project.as_ref()
        .or(config.app.as_ref())
        .context("Cannot resolve build node: set build.node, target, or project in ops.toml")?;
    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;
    let nodes = api::list_nodes_v2(&token).await?;
    let node = nodes.nodes.iter()
        .find(|n| n.bound_apps.as_ref().map_or(false, |apps|
            apps.iter().any(|a| &a.project_name == project)))
        .with_context(|| format!("No nodes found for project '{}'. Set build.node explicitly.", project))?;
    Ok(node.domain.clone())
}

/// ops build 主入口
pub async fn handle_build(
    file: String,
    git_ref: Option<String>,
    service_filter: Option<String>,
    tag: Option<String>,
    no_push: bool,
) -> Result<()> {
    let total_start = Instant::now();

    // 1. 加载配置
    println!("{}", "📦 Reading ops.toml [build]...".cyan());
    let config = load_ops_toml(&file)?;
    let build = config.build.as_ref()
        .context("ops.toml missing [build] section. Add a [build] section to enable remote builds.")?;

    let node = resolve_build_node(&config, build).await?;
    println!("   Node: {}", node.cyan());
    println!("   Path: {}", build.path.green());
    println!("   Command: {}", build.command.yellow());

    // 2. 确保远程目录存在
    println!("\n{}", "🔑 Connecting to build node...".cyan());
    ssh::execute_remote_command(&node, &format!("mkdir -p {}", build.path), None).await?;

    // 3. 同步代码
    sync_code(build, &node, &git_ref).await?;

    // 4. 执行构建命令
    println!("\n{}", "🔨 Running build...".cyan());
    let build_start = Instant::now();
    let build_cmd = format!("cd {} && {}", build.path, build.command);
    ssh::execute_remote_command(&node, &build_cmd, None).await?;
    let build_duration = build_start.elapsed();
    println!("   {} ({})", "✔ Build complete".green(), format_duration(build_duration));

    // 5. 构建并推送 Docker 镜像（如果配置了 [build.image]）
    if let Some(image_config) = &build.image {
        build_and_push_images(build, &node, image_config, &service_filter, &tag, no_push).await?;
    }

    // 6. 输出总结
    let total_duration = total_start.elapsed();
    println!(
        "\n{} Build finished in {}",
        "✅".green(),
        format_duration(total_duration).cyan(),
    );

    Ok(())
}

/// 同步代码到构建节点
async fn sync_code(build: &BuildConfig, node: &str, git_ref: &Option<String>) -> Result<()> {
    match build.source.as_str() {
        "git" => {
            println!("\n{}", "📤 Syncing code (git)...".cyan());
            let git = build.git.as_ref()
                .context("build.source='git' requires [build.git] section")?;

            let ref_or_branch = git_ref.as_deref()
                .or(build.branch.as_deref())
                .unwrap_or("main");

            // 检查远程是否已有 .git 目录
            let check = format!(
                "test -d {}/.git && echo 'exists' || echo 'missing'",
                build.path
            );
            let output = ssh::execute_remote_command_with_output(node, &check).await?;
            let output_str = String::from_utf8_lossy(&output).trim().to_string();

            if output_str == "exists" {
                // 已有仓库 → fetch + checkout
                let cmd = if git_ref.is_some() {
                    // 指定了具体 ref（如 commit SHA）→ fetch all + checkout
                    format!(
                        "cd {} && git fetch origin && git checkout {} && git reset --hard {}",
                        build.path, ref_or_branch, ref_or_branch
                    )
                } else {
                    // 默认分支 → pull
                    format!(
                        "cd {} && git fetch origin && git checkout {} && git pull origin {}",
                        build.path, ref_or_branch, ref_or_branch
                    )
                };
                ssh::execute_remote_command(node, &cmd, None).await?;
            } else {
                // 初次 clone
                if let Some(key_path) = &git.ssh_key {
                    let expanded = shellexpand::tilde(key_path).to_string();
                    setup_deploy_key(node, &expanded).await?;
                }
                let cmd = format!(
                    "GIT_SSH_COMMAND='ssh -o StrictHostKeyChecking=no' git clone {} {} && cd {} && git checkout {}",
                    git.repo, build.path, build.path, ref_or_branch
                );
                ssh::execute_remote_command(node, &cmd, None).await?;
            }
            println!("   {} (ref: {})", "✔ Code synced".green(), ref_or_branch.yellow());
        }
        "push" => {
            println!("\n{}", "📤 Syncing code (rsync)...".cyan());
            // 使用 rsync 推送本地文件到构建节点
            rsync_push_to_build(node, &build.path).await?;
            println!("   {}", "✔ Code synced".green());
        }
        other => return Err(anyhow::anyhow!("Unknown build source: {}", other)),
    }
    Ok(())
}

/// 上传 deploy key 到服务器并配置 SSH
async fn setup_deploy_key(target: &str, local_key_path: &str) -> Result<()> {
    let key_content = std::fs::read_to_string(local_key_path)
        .with_context(|| format!("Cannot read deploy key: {}", local_key_path))?;

    ssh::execute_remote_command(
        target,
        "mkdir -p ~/.ssh && cat > ~/.ssh/deploy_key && chmod 600 ~/.ssh/deploy_key",
        Some(&key_content),
    ).await?;

    ssh::execute_remote_command(
        target,
        r#"grep -q 'deploy_key' ~/.ssh/config 2>/dev/null || cat >> ~/.ssh/config << 'SSHEOF'
Host github.com
  IdentityFile ~/.ssh/deploy_key
  StrictHostKeyChecking no
SSHEOF
chmod 600 ~/.ssh/config"#,
        None,
    ).await?;

    println!("   {}", "✔ Deploy key configured".green());
    Ok(())
}

/// rsync 本地代码到构建节点
async fn rsync_push_to_build(target_str: &str, build_path: &str) -> Result<()> {
    use crate::{api, config, utils};
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let target = utils::parse_target_v2(target_str)?;
    let full_domain = target.domain();

    let cfg = config::load_config().context("Config error")?;
    let token = cfg.token.context("Please run `ops login` first.")?;

    // Get CI key based on target type
    let private_key = match &target {
        utils::TargetType::NodeId { id, .. } => {
            api::get_node_ci_key(&token, *id).await?.private_key
        }
        utils::TargetType::AppTarget { app, project, .. } => {
            api::get_app_ci_key(&token, project, app).await?.private_key
        }
    };

    let mut temp_key_file = tempfile::NamedTempFile::new()?;
    writeln!(temp_key_file, "{}", private_key)?;
    let meta = temp_key_file.as_file().metadata()?;
    let mut perms = meta.permissions();
    perms.set_mode(0o600);
    temp_key_file.as_file().set_permissions(perms)?;
    let key_path = temp_key_file.path().to_str().unwrap().to_string();

    let ssh_cmd = format!(
        "ssh -i {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null",
        key_path
    );
    let remote = format!("root@{}:{}/", full_domain, build_path);

    let status = std::process::Command::new("rsync")
        .arg("-az")
        .arg("--delete")
        .arg("-e").arg(&ssh_cmd)
        .arg("--exclude").arg("target/")
        .arg("--exclude").arg("node_modules/")
        .arg("--exclude").arg(".git/")
        .arg("--exclude").arg(".env")
        .arg("--exclude").arg(".env.deploy")
        .arg("./")
        .arg(&remote)
        .status()
        .context("Failed to execute rsync (is rsync installed?)")?;

    if !status.success() {
        return Err(anyhow::anyhow!("rsync failed with status: {}", status));
    }
    Ok(())
}

/// 构建并推送 Docker 镜像
async fn build_and_push_images(
    build: &BuildConfig,
    node: &str,
    image_config: &crate::types::BuildImageConfig,
    service_filter: &Option<String>,
    tag: &Option<String>,
    no_push: bool,
) -> Result<()> {
    let tag = tag.as_deref().unwrap_or("latest");
    let services: Vec<&str> = if let Some(filter) = service_filter {
        vec![filter.as_str()]
    } else {
        image_config.services.iter().map(|s| s.as_str()).collect()
    };

    println!(
        "\n{} ({} services, tag: {})",
        "🐳 Building Docker images...".cyan(),
        services.len().to_string().yellow(),
        tag.yellow(),
    );

    // Docker registry login
    let token = resolve_env_value(&image_config.token)?;
    let login_cmd = format!(
        "echo '{}' | docker login {} -u {} --password-stdin 2>/dev/null",
        token, image_config.registry, image_config.username,
    );
    ssh::execute_remote_command(node, &login_cmd, None).await?;
    println!("   {}", "✔ Registry login".green());

    // Build & push each service
    let img_start = Instant::now();
    for (i, svc) in services.iter().enumerate() {
        let progress = format!("[{}/{}]", i + 1, services.len());
        println!("   {} {} {}/{}", progress.dimmed(), "📦".dimmed(), image_config.prefix, svc);

        // Build image
        let build_cmd = format!(
            "cd {} && docker build -f {} --build-arg {}={} -t {}/{}:{} -t {}/{}:latest . 2>&1 | tail -1",
            build.path,
            image_config.dockerfile,
            image_config.binary_arg, svc,
            image_config.prefix, svc, tag,
            image_config.prefix, svc,
        );
        ssh::execute_remote_command(node, &build_cmd, None).await
            .with_context(|| format!("Failed to build image for {}", svc))?;

        // Push image
        if !no_push {
            let push_cmd = format!(
                "docker push {}/{}:{} && docker push {}/{}:latest",
                image_config.prefix, svc, tag,
                image_config.prefix, svc,
            );
            ssh::execute_remote_command(node, &push_cmd, None).await
                .with_context(|| format!("Failed to push image for {}", svc))?;
        }
    }

    let img_duration = img_start.elapsed();
    let action = if no_push { "built" } else { "built & pushed" };
    println!(
        "   {} {} {} images {} ({})",
        "✔".green(),
        services.len(),
        "service".green(),
        action,
        format_duration(img_duration),
    );

    // Clean up dangling images
    ssh::execute_remote_command(node, "docker image prune -f 2>/dev/null", None).await.ok();

    Ok(())
}

fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else {
        format!("{}m{}s", secs / 60, secs % 60)
    }
}
