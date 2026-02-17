use super::{DockerStage, Framework, SourceInfo};
use anyhow::Result;
use std::fs;
use std::path::Path;

/// Detect package manager from lockfiles
fn detect_package_manager(dir: &Path) -> (String, String) {
    if dir.join("bun.lockb").exists() || dir.join("bun.lock").exists() {
        ("bun".into(), "bun install --frozen-lockfile".into())
    } else if dir.join("pnpm-lock.yaml").exists() {
        ("pnpm".into(), "pnpm install --frozen-lockfile".into())
    } else if dir.join("yarn.lock").exists() {
        ("yarn".into(), "yarn install --frozen-lockfile".into())
    } else {
        ("npm".into(), "npm ci".into())
    }
}

/// Detect Node.js version from .nvmrc, .node-version, or package.json engines
fn detect_node_version(dir: &Path, pkg: &serde_json::Value) -> String {
    // .nvmrc
    if let Ok(v) = fs::read_to_string(dir.join(".nvmrc")) {
        let v = v.trim().trim_start_matches('v');
        if let Some(major) = v.split('.').next() {
            if major.parse::<u32>().is_ok() {
                return major.to_string();
            }
        }
    }
    // .node-version
    if let Ok(v) = fs::read_to_string(dir.join(".node-version")) {
        let v = v.trim().trim_start_matches('v');
        if let Some(major) = v.split('.').next() {
            if major.parse::<u32>().is_ok() {
                return major.to_string();
            }
        }
    }
    // package.json engines.node
    if let Some(engines) = pkg.get("engines") {
        if let Some(node_ver) = engines.get("node").and_then(|v| v.as_str()) {
            // Parse ">=18", "^20", "20.x", "20" etc — extract first number
            let digits: String = node_ver.chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                return digits;
            }
        }
    }
    "22".to_string()
}

/// Copy lockfile instruction based on package manager
fn lockfile_copy(pm: &str) -> &'static str {
    match pm {
        "bun" => "COPY package.json bun.lockb* bun.lock* ./",
        "pnpm" => "COPY package.json pnpm-lock.yaml* ./",
        "yarn" => "COPY package.json yarn.lock* ./",
        _ => "COPY package.json package-lock.json* ./",
    }
}

/// Default .dockerignore for Node projects
fn node_dockerignore() -> Vec<String> {
    vec![
        "node_modules".into(),
        ".next".into(),
        ".nuxt".into(),
        ".output".into(),
        "dist".into(),
        ".git".into(),
        ".env*".into(),
        "*.md".into(),
        ".vscode".into(),
        ".idea".into(),
    ]
}

fn has_dep(pkg: &serde_json::Value, name: &str) -> bool {
    pkg.get("dependencies")
        .and_then(|d| d.get(name))
        .is_some()
}

fn has_dev_dep(pkg: &serde_json::Value, name: &str) -> bool {
    pkg.get("devDependencies")
        .and_then(|d| d.get(name))
        .is_some()
}

fn read_package_json(dir: &Path) -> Option<serde_json::Value> {
    let path = dir.join("package.json");
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Detect port from scripts (look for --port or -p flags)
fn detect_port_from_scripts(pkg: &serde_json::Value) -> Option<u16> {
    if let Some(scripts) = pkg.get("scripts").and_then(|s| s.as_object()) {
        for key in &["start", "dev", "serve"] {
            if let Some(cmd) = scripts.get(*key).and_then(|v| v.as_str()) {
                // Look for --port NNNN or -p NNNN
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                for (i, part) in parts.iter().enumerate() {
                    if (*part == "--port" || *part == "-p") && i + 1 < parts.len() {
                        if let Ok(port) = parts[i + 1].parse::<u16>() {
                            return Some(port);
                        }
                    }
                    // --port=NNNN
                    if let Some(val) = part.strip_prefix("--port=") {
                        if let Ok(port) = val.parse::<u16>() {
                            return Some(port);
                        }
                    }
                }
            }
        }
    }
    None
}

// ─── Next.js ──────────────────────────────────────────────────────

pub fn scan_nextjs(dir: &Path) -> Result<Option<SourceInfo>> {
    let pkg = match read_package_json(dir) {
        Some(p) => p,
        None => return Ok(None),
    };

    if !has_dep(&pkg, "next") {
        return Ok(None);
    }

    let (pm, install_cmd) = detect_package_manager(dir);
    let node_ver = detect_node_version(dir, &pkg);
    let base = format!("node:{}-alpine", node_ver);
    let port = detect_port_from_scripts(&pkg).unwrap_or(3000);

    let stages = vec![
        // Stage 1: deps
        DockerStage {
            name: Some("deps".into()),
            base_image: base.clone(),
            workdir: "/app".into(),
            instructions: vec![
                lockfile_copy(&pm).into(),
                format!("RUN {}", install_cmd),
            ],
            expose: None,
            cmd: None,
        },
        // Stage 2: builder
        DockerStage {
            name: Some("builder".into()),
            base_image: base.clone(),
            workdir: "/app".into(),
            instructions: vec![
                "COPY --from=deps /app/node_modules ./node_modules".into(),
                "COPY . .".into(),
                format!("RUN {} run build", if pm == "bun" { "bun" } else { &pm }),
            ],
            expose: None,
            cmd: None,
        },
        // Stage 3: runner
        DockerStage {
            name: None,
            base_image: base,
            workdir: "/app".into(),
            instructions: vec![
                "ENV NODE_ENV=production".into(),
                "COPY --from=builder /app/public ./public".into(),
                "COPY --from=builder /app/.next/standalone ./".into(),
                "COPY --from=builder /app/.next/static ./.next/static".into(),
            ],
            expose: Some(port),
            cmd: Some(vec!["node".into(), "server.js".into()]),
        },
    ];

    let mut notes = vec![];
    // Check if next.config has standalone output
    let has_standalone = check_next_standalone(dir);
    if !has_standalone {
        notes.push("Add `output: 'standalone'` to next.config.js/ts for optimal Docker builds".into());
    }

    Ok(Some(SourceInfo {
        family: "Next.js".into(),
        framework: Framework::NextJs,
        version: Some(node_ver),
        port,
        env_vars: vec![("NODE_ENV".into(), "production".into())],
        build_args: vec![],
        install_cmd: install_cmd.clone(),
        build_cmd: Some(format!("{} run build", if pm == "bun" { "bun" } else { &pm })),
        start_cmd: "node server.js".into(),
        binary_name: None,
        entry_point: None,
        package_manager: Some(pm),
        has_lockfile: true,
        dockerfile_stages: stages,
        dockerignore_entries: node_dockerignore(),
        notes,
    }))
}

fn check_next_standalone(dir: &Path) -> bool {
    for name in &["next.config.js", "next.config.ts", "next.config.mjs"] {
        if let Ok(content) = fs::read_to_string(dir.join(name)) {
            if content.contains("standalone") {
                return true;
            }
        }
    }
    false
}

// ─── Nuxt ─────────────────────────────────────────────────────────

pub fn scan_nuxtjs(dir: &Path) -> Result<Option<SourceInfo>> {
    let pkg = match read_package_json(dir) {
        Some(p) => p,
        None => return Ok(None),
    };

    if !has_dep(&pkg, "nuxt") {
        return Ok(None);
    }

    let (pm, install_cmd) = detect_package_manager(dir);
    let node_ver = detect_node_version(dir, &pkg);
    let base = format!("node:{}-alpine", node_ver);
    let port = detect_port_from_scripts(&pkg).unwrap_or(3000);

    let stages = vec![
        DockerStage {
            name: Some("builder".into()),
            base_image: base.clone(),
            workdir: "/app".into(),
            instructions: vec![
                lockfile_copy(&pm).into(),
                format!("RUN {}", install_cmd),
                "COPY . .".into(),
                format!("RUN {} run build", if pm == "bun" { "bun" } else { &pm }),
            ],
            expose: None,
            cmd: None,
        },
        DockerStage {
            name: None,
            base_image: base,
            workdir: "/app".into(),
            instructions: vec![
                "COPY --from=builder /app/.output ./".into(),
            ],
            expose: Some(port),
            cmd: Some(vec!["node".into(), "server/index.mjs".into()]),
        },
    ];

    Ok(Some(SourceInfo {
        family: "Nuxt".into(),
        framework: Framework::NuxtJs,
        version: Some(node_ver),
        port,
        env_vars: vec![("NODE_ENV".into(), "production".into())],
        build_args: vec![],
        install_cmd,
        build_cmd: Some(format!("{} run build", if pm == "bun" { "bun" } else { &pm })),
        start_cmd: "node server/index.mjs".into(),
        binary_name: None,
        entry_point: None,
        package_manager: Some(pm),
        has_lockfile: true,
        dockerfile_stages: stages,
        dockerignore_entries: node_dockerignore(),
        notes: vec![],
    }))
}

// ─── Remix ────────────────────────────────────────────────────────

pub fn scan_remix(dir: &Path) -> Result<Option<SourceInfo>> {
    let pkg = match read_package_json(dir) {
        Some(p) => p,
        None => return Ok(None),
    };

    let is_remix = has_dep(&pkg, "@remix-run/node")
        || has_dep(&pkg, "@remix-run/react")
        || has_dep(&pkg, "remix");

    if !is_remix {
        return Ok(None);
    }

    let (pm, install_cmd) = detect_package_manager(dir);
    let node_ver = detect_node_version(dir, &pkg);
    let base = format!("node:{}-alpine", node_ver);
    let port = detect_port_from_scripts(&pkg).unwrap_or(3000);
    let run_prefix = if pm == "bun" { "bun" } else { &pm };

    let stages = vec![
        DockerStage {
            name: Some("deps".into()),
            base_image: base.clone(),
            workdir: "/app".into(),
            instructions: vec![
                lockfile_copy(&pm).into(),
                format!("RUN {}", install_cmd),
            ],
            expose: None,
            cmd: None,
        },
        DockerStage {
            name: Some("builder".into()),
            base_image: base.clone(),
            workdir: "/app".into(),
            instructions: vec![
                "COPY --from=deps /app/node_modules ./node_modules".into(),
                "COPY . .".into(),
                format!("RUN {} run build", run_prefix),
            ],
            expose: None,
            cmd: None,
        },
        DockerStage {
            name: None,
            base_image: base,
            workdir: "/app".into(),
            instructions: vec![
                "ENV NODE_ENV=production".into(),
                "COPY --from=deps /app/node_modules ./node_modules".into(),
                "COPY --from=builder /app/build ./build".into(),
                "COPY --from=builder /app/public ./public".into(),
                "COPY --from=builder /app/package.json ./".into(),
            ],
            expose: Some(port),
            cmd: Some(vec![run_prefix.to_string(), "run".into(), "start".into()]),
        },
    ];

    Ok(Some(SourceInfo {
        family: "Remix".into(),
        framework: Framework::RemixJs,
        version: Some(node_ver),
        port,
        env_vars: vec![("NODE_ENV".into(), "production".into())],
        build_args: vec![],
        install_cmd,
        build_cmd: Some(format!("{} run build", run_prefix)),
        start_cmd: format!("{} run start", run_prefix),
        binary_name: None,
        entry_point: None,
        package_manager: Some(pm),
        has_lockfile: true,
        dockerfile_stages: stages,
        dockerignore_entries: node_dockerignore(),
        notes: vec![],
    }))
}

// ─── Vite SPA ─────────────────────────────────────────────────────

pub fn scan_vite_spa(dir: &Path) -> Result<Option<SourceInfo>> {
    let pkg = match read_package_json(dir) {
        Some(p) => p,
        None => return Ok(None),
    };

    let has_vite = has_dev_dep(&pkg, "vite") || has_dep(&pkg, "vite");
    if !has_vite {
        return Ok(None);
    }

    // If it also has a server framework, it's not a pure SPA
    if has_dep(&pkg, "next") || has_dep(&pkg, "nuxt") || has_dep(&pkg, "@remix-run/node")
        || has_dep(&pkg, "express") || has_dep(&pkg, "fastify") || has_dep(&pkg, "hono")
    {
        return Ok(None);
    }

    let (pm, install_cmd) = detect_package_manager(dir);
    let node_ver = detect_node_version(dir, &pkg);
    let base = format!("node:{}-alpine", node_ver);
    let run_prefix = if pm == "bun" { "bun" } else { &pm };

    let stages = vec![
        DockerStage {
            name: Some("builder".into()),
            base_image: base,
            workdir: "/app".into(),
            instructions: vec![
                lockfile_copy(&pm).into(),
                format!("RUN {}", install_cmd),
                "COPY . .".into(),
                format!("RUN {} run build", run_prefix),
            ],
            expose: None,
            cmd: None,
        },
        DockerStage {
            name: None,
            base_image: "nginx:alpine".into(),
            workdir: "/usr/share/nginx/html".into(),
            instructions: vec![
                "COPY --from=builder /app/dist .".into(),
            ],
            expose: Some(80),
            cmd: Some(vec!["nginx".into(), "-g".into(), "daemon off;".into()]),
        },
    ];

    Ok(Some(SourceInfo {
        family: "Vite SPA".into(),
        framework: Framework::ViteSpa,
        version: Some(node_ver),
        port: 80,
        env_vars: vec![],
        build_args: vec![],
        install_cmd,
        build_cmd: Some(format!("{} run build", run_prefix)),
        start_cmd: "nginx -g 'daemon off;'".into(),
        binary_name: None,
        entry_point: None,
        package_manager: Some(pm),
        has_lockfile: true,
        dockerfile_stages: stages,
        dockerignore_entries: node_dockerignore(),
        notes: vec![],
    }))
}

// ─── Generic Node.js ──────────────────────────────────────────────

pub fn scan_generic(dir: &Path) -> Result<Option<SourceInfo>> {
    let pkg = match read_package_json(dir) {
        Some(p) => p,
        None => return Ok(None),
    };

    let (pm, install_cmd) = detect_package_manager(dir);
    let node_ver = detect_node_version(dir, &pkg);
    let base = format!("node:{}-alpine", node_ver);
    let port = detect_port_from_scripts(&pkg).unwrap_or(3000);
    let run_prefix = if pm == "bun" { "bun" } else { &pm };

    // Determine start command
    let start_cmd = if let Some(scripts) = pkg.get("scripts").and_then(|s| s.as_object()) {
        if scripts.contains_key("start") {
            format!("{} run start", run_prefix)
        } else if let Some(main) = pkg.get("main").and_then(|v| v.as_str()) {
            format!("node {}", main)
        } else {
            "node index.js".into()
        }
    } else if let Some(main) = pkg.get("main").and_then(|v| v.as_str()) {
        format!("node {}", main)
    } else {
        "node index.js".into()
    };

    let has_build = pkg.get("scripts")
        .and_then(|s| s.get("build"))
        .is_some();

    let mut instructions = vec![
        lockfile_copy(&pm).into(),
        format!("RUN {}", install_cmd),
        "COPY . .".to_string(),
    ];

    if has_build {
        instructions.push(format!("RUN {} run build", run_prefix));
    }

    let stages = vec![
        DockerStage {
            name: None,
            base_image: base,
            workdir: "/app".into(),
            instructions,
            expose: Some(port),
            cmd: Some(start_cmd.split_whitespace().map(String::from).collect()),
        },
    ];

    Ok(Some(SourceInfo {
        family: "Node.js".into(),
        framework: Framework::GenericNode,
        version: Some(node_ver),
        port,
        env_vars: vec![("NODE_ENV".into(), "production".into())],
        build_args: vec![],
        install_cmd,
        build_cmd: if has_build { Some(format!("{} run build", run_prefix)) } else { None },
        start_cmd,
        binary_name: None,
        entry_point: None,
        package_manager: Some(pm),
        has_lockfile: true,
        dockerfile_stages: stages,
        dockerignore_entries: node_dockerignore(),
        notes: vec![],
    }))
}
