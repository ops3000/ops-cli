use crate::commands::common::resolve_env_value;
use crate::commands::deploy::load_ops_toml;
use crate::commands::ssh::SshSession;
use crate::types::{BuildConfig, OpsToml};
use crate::{api, config};
use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;
use std::time::Instant;

/// 上传 SSH key 到构建节点，按项目隔离: ~/.ssh/{project_name}/{key_filename}
fn setup_build_ssh_key(session: &SshSession, local_key_path: &str, project_name: &str) -> Result<()> {
    let key_content = fs::read_to_string(local_key_path)
        .with_context(|| format!("Cannot read SSH key: {}", local_key_path))?;

    let key_filename = Path::new(local_key_path)
        .file_name()
        .context("Invalid key path")?
        .to_str()
        .context("Invalid key filename")?;

    let remote_key_dir = format!("~/.ssh/{}", project_name);
    let remote_key_path = format!("{}/{}", remote_key_dir, key_filename);

    // 上传 key
    session.exec(
        &format!("mkdir -p {} && cat > {} && chmod 600 {}", remote_key_dir, remote_key_path, remote_key_path),
        Some(&key_content),
    )?;

    // 配置 ~/.ssh/config
    session.exec(
        &format!(
            r#"grep -q '{}' ~/.ssh/config 2>/dev/null || cat >> ~/.ssh/config << 'SSHEOF'
Host github.com
  Hostname ssh.github.com
  Port 443
  User git
  IdentityFile {}
  IdentitiesOnly yes
  StrictHostKeyChecking no
SSHEOF
chmod 600 ~/.ssh/config"#,
            remote_key_path, remote_key_path
        ),
        None,
    )?;

    o_success!("   {} ({})", "✔ SSH key configured".green(), remote_key_path);
    Ok(())
}

/// 解析构建节点，优先级：build.node → config.target → API 自动查询
async fn resolve_build_node(config: &OpsToml, build: &BuildConfig) -> Result<String> {
    if let Some(id) = build.node {
        return Ok(id.to_string());
    }
    if let Some(ref t) = config.target {
        return Ok(t.clone());
    }
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
    jobs: u8,
) -> Result<()> {
    let total_start = Instant::now();
    let jobs = jobs.max(1) as usize;

    // 1. 加载配置
    o_step!("{}", "📦 Reading ops.toml [build]...".cyan());
    let config = load_ops_toml(&file)?;
    let build = config.build.as_ref()
        .context("ops.toml missing [build] section. Add a [build] section to enable remote builds.")?;

    let node = resolve_build_node(&config, build).await?;
    o_detail!("   Node: {}", node.cyan());
    o_detail!("   Path: {}", build.path.green());
    o_detail!("   Command: {}", build.command.yellow());

    // 2. 建立 SSH 会话（只 fetch 一次 CI key）
    o_step!("\n{}", "🔑 Connecting to build node...".cyan());
    let session = SshSession::connect(&node).await?;
    session.exec(&format!("mkdir -p {}", build.path), None)?;

    // 3. 同步代码
    let project_name = config.project.as_ref()
        .or(config.app.as_ref())
        .context("ops.toml must have 'project' or 'app'")?;
    sync_code(build, &session, &node, &git_ref, project_name).await?;

    // 4. 执行构建命令
    o_step!("\n{}", "🔨 Running build...".cyan());
    let build_start = Instant::now();
    let build_cmd = format!("source $HOME/.cargo/env 2>/dev/null; cd {} && {}", build.path, build.command);
    session.exec(&build_cmd, None)?;
    let build_duration = build_start.elapsed();
    o_success!("   {} ({})", "✔ Build complete".green(), format_duration(build_duration));

    // 5. 构建并推送 Docker 镜像（如果配置了 [build.image]）
    if let Some(image_config) = &build.image {
        build_and_push_images(build, &session, image_config, &service_filter, &tag, no_push, jobs)?;
    }

    // 6. 输出总结
    let total_duration = total_start.elapsed();
    o_result!(
        "\n{} Build finished in {}",
        "✅".green(),
        format_duration(total_duration).cyan(),
    );

    Ok(())
}

/// 同步代码到构建节点
async fn sync_code(build: &BuildConfig, session: &SshSession, node: &str, git_ref: &Option<String>, project_name: &str) -> Result<()> {
    match build.source.as_str() {
        "git" => {
            o_step!("\n{}", "📤 Syncing code (git)...".cyan());
            let git = build.git.as_ref()
                .context("build.source='git' requires [build.git] section")?;

            let ref_or_branch = git_ref.as_deref()
                .or(build.branch.as_deref())
                .unwrap_or("main");

            // 配置认证（如果需要）
            if let Some(key_path) = &git.ssh_key {
                let expanded = shellexpand::tilde(key_path).to_string();
                setup_build_ssh_key(session, &expanded, project_name)?;
            }

            // 检查远程是否已有 .git 目录
            let check = format!(
                "test -d {}/.git && echo 'exists' || echo 'missing'",
                build.path
            );
            let output = session.exec_output(&check)?;
            let output_str = String::from_utf8_lossy(&output).trim().to_string();

            // 构建 clone URL（token 方式需要注入到 URL）
            let repo_url = if let Some(token_val) = &git.token {
                let token = resolve_env_value(token_val)?;
                let https_url = git.repo
                    .replace("git@github.com:", "https://github.com/")
                    .replace(".git", "");
                format!("https://x-access-token:{}@{}", token, https_url.trim_start_matches("https://"))
            } else {
                git.repo.clone()
            };

            if output_str == "exists" {
                let cmd = if git_ref.is_some() {
                    format!(
                        "cd {} && git fetch origin && git checkout {} && git reset --hard {}",
                        build.path, ref_or_branch, ref_or_branch
                    )
                } else {
                    format!(
                        "cd {} && git fetch origin && git checkout {} && git pull origin {}",
                        build.path, ref_or_branch, ref_or_branch
                    )
                };
                session.exec(&cmd, None)?;
            } else {
                let ssh_opts = if git.token.is_none() && git.ssh_key.is_none() {
                    "GIT_SSH_COMMAND='ssh -o StrictHostKeyChecking=no' "
                } else if git.token.is_some() {
                    ""
                } else {
                    "GIT_SSH_COMMAND='ssh -o StrictHostKeyChecking=no' "
                };
                let cmd = format!(
                    "{}git clone {} {} && cd {} && git checkout {}",
                    ssh_opts, repo_url, build.path, build.path, ref_or_branch
                );
                session.exec(&cmd, None)?;
            }
            o_success!("   {} (ref: {})", "✔ Code synced".green(), ref_or_branch.yellow());
        }
        "push" => {
            o_step!("\n{}", "📤 Syncing code (rsync)...".cyan());
            session.rsync_push(&build.path)?;
            o_success!("   {}", "✔ Code synced".green());
        }
        other => return Err(anyhow::anyhow!("Unknown build source: {}", other)),
    }
    Ok(())
}

/// 构建并推送 Docker 镜像
fn build_and_push_images(
    build: &BuildConfig,
    session: &SshSession,
    image_config: &crate::types::BuildImageConfig,
    service_filter: &Option<String>,
    tag: &Option<String>,
    no_push: bool,
    jobs: usize,
) -> Result<()> {
    let tag = tag.as_deref().unwrap_or("latest");
    let services: Vec<&str> = if let Some(filter) = service_filter {
        vec![filter.as_str()]
    } else {
        image_config.services.iter().map(|s| s.as_str()).collect()
    };

    o_step!(
        "\n{} ({} services, tag: {}, jobs: {})",
        "🐳 Building Docker images...".cyan(),
        services.len().to_string().yellow(),
        tag.yellow(),
        jobs.to_string().yellow(),
    );

    // Docker registry login
    let token = resolve_env_value(&image_config.token)?;
    let login_cmd = format!(
        "echo '{}' | docker login {} -u {} --password-stdin 2>/dev/null",
        token, image_config.registry, image_config.username,
    );
    session.exec(&login_cmd, None)?;
    o_success!("   {}", "✔ Registry login".green());

    let img_start = Instant::now();

    if jobs <= 1 {
        // 顺序构建（兼容旧行为）
        for (i, svc) in services.iter().enumerate() {
            let progress = format!("[{}/{}]", i + 1, services.len());
            o_detail!("   {} {} {}/{}", progress.dimmed(), "📦".dimmed(), image_config.prefix, svc);

            let build_cmd = format!(
                "cd {} && docker build -f {} --build-arg {}={} -t {}/{}:{} -t {}/{}:latest .",
                build.path, image_config.dockerfile,
                image_config.binary_arg, svc,
                image_config.prefix, svc, tag,
                image_config.prefix, svc,
            );
            session.exec(&build_cmd, None)
                .with_context(|| format!("Failed to build image for {}", svc))?;

            if !no_push {
                let push_cmd = format!(
                    "docker push {}/{}:{} && docker push {}/{}:latest",
                    image_config.prefix, svc, tag,
                    image_config.prefix, svc,
                );
                session.exec(&push_cmd, None)
                    .with_context(|| format!("Failed to push image for {}", svc))?;
            }
        }
    } else {
        // 并行构建：按 batch 分组，每 batch 在远程 shell 并行执行
        let batches: Vec<&[&str]> = services.chunks(jobs).collect();
        let total_batches = batches.len();

        for (batch_idx, batch) in batches.iter().enumerate() {
            let batch_names: Vec<&str> = batch.to_vec();
            o_detail!(
                "   {} Building {}...",
                format!("[batch {}/{}]", batch_idx + 1, total_batches).dimmed(),
                batch_names.join(", ").cyan(),
            );

            // 构建并行 shell 命令：每个 service 后台运行，输出到 log，exit code 到文件
            let mut cmds = Vec::new();
            for svc in &batch_names {
                cmds.push(format!(
                    "(cd {} && docker build -f {} --build-arg {}={} -t {}/{}:{} -t {}/{}:latest . > /tmp/ops_build_{}.log 2>&1; echo $? > /tmp/ops_build_{}.exit) &",
                    build.path, image_config.dockerfile,
                    image_config.binary_arg, svc,
                    image_config.prefix, svc, tag,
                    image_config.prefix, svc,
                    svc, svc,
                ));
            }
            cmds.push("wait".to_string());
            let parallel_cmd = cmds.join("\n");
            session.exec(&parallel_cmd, None)?;

            // 检查每个 service 的构建结果
            let exit_check: Vec<String> = batch_names.iter()
                .map(|svc| format!("echo -n \"{}:\"; cat /tmp/ops_build_{}.exit", svc, svc))
                .collect();
            let check_cmd = exit_check.join("; ");
            let output = session.exec_output(&check_cmd)?;
            let results = String::from_utf8_lossy(&output);

            let mut failed: Vec<String> = Vec::new();
            for line in results.trim().split('\n') {
                if let Some((svc, code)) = line.split_once(':') {
                    let svc = svc.trim();
                    let code = code.trim();
                    if code == "0" {
                        o_success!("   {} {}", "✔".green(), svc);
                    } else {
                        o_error!("   {} {} (exit {})", "✗".red(), svc.red(), code);
                        failed.push(svc.to_string());
                    }
                }
            }

            if !failed.is_empty() {
                // 显示失败 service 的构建日志
                for svc in &failed {
                    o_error!("\n   --- {} build log ---", svc);
                    let log_cmd = format!("tail -30 /tmp/ops_build_{}.log", svc);
                    session.exec(&log_cmd, None).ok();
                }
                return Err(anyhow::anyhow!("Build failed for: {}", failed.join(", ")));
            }
        }

        // 并行推送
        if !no_push {
            o_detail!("   {}", "Pushing images...".dimmed());
            let push_batches: Vec<&[&str]> = services.chunks(jobs).collect();
            for batch in &push_batches {
                let mut push_cmds = Vec::new();
                for svc in *batch {
                    push_cmds.push(format!(
                        "(docker push {}/{}:{} && docker push {}/{}:latest > /tmp/ops_push_{}.log 2>&1; echo $? > /tmp/ops_push_{}.exit) &",
                        image_config.prefix, svc, tag,
                        image_config.prefix, svc,
                        svc, svc,
                    ));
                }
                push_cmds.push("wait".to_string());
                session.exec(&push_cmds.join("\n"), None)?;

                // 检查 push 结果
                let exit_check: Vec<String> = batch.iter()
                    .map(|svc| format!("echo -n \"{}:\"; cat /tmp/ops_push_{}.exit", svc, svc))
                    .collect();
                let output = session.exec_output(&exit_check.join("; "))?;
                let results = String::from_utf8_lossy(&output);

                for line in results.trim().split('\n') {
                    if let Some((svc, code)) = line.split_once(':') {
                        if code.trim() != "0" {
                            return Err(anyhow::anyhow!("Push failed for: {}", svc.trim()));
                        }
                    }
                }
            }
            o_success!("   {}", "✔ All images pushed".green());
        }
    }

    let img_duration = img_start.elapsed();
    let action = if no_push { "built" } else { "built & pushed" };
    o_success!(
        "   {} {} {} images {} ({})",
        "✔".green(),
        services.len(),
        "service".green(),
        action,
        format_duration(img_duration),
    );

    session.exec("docker image prune -f 2>/dev/null", None).ok();

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
