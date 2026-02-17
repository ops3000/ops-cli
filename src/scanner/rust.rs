use super::{DockerStage, Framework, SourceInfo};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn scan(dir: &Path) -> Result<Option<SourceInfo>> {
    let cargo_toml_path = dir.join("Cargo.toml");
    if !cargo_toml_path.exists() {
        return Ok(None);
    }

    let cargo_toml = fs::read_to_string(&cargo_toml_path)?;
    let binary_name = detect_binary_name(&cargo_toml);
    let has_lock = dir.join("Cargo.lock").exists();

    let stages = vec![
        // Builder: dependency cache + release build
        DockerStage {
            name: Some("builder".into()),
            base_image: "rust:1-slim".into(),
            workdir: "/app".into(),
            instructions: vec![
                // Dependency cache trick: build dummy project first
                format!("COPY Cargo.toml Cargo.lock{} ./", if has_lock { "" } else { "*" }),
                "RUN mkdir src && echo 'fn main(){}' > src/main.rs && cargo build --release && rm -rf src".into(),
                "COPY . .".into(),
                "RUN cargo build --release".into(),
            ],
            expose: None,
            cmd: None,
        },
        // Runtime: minimal debian-slim
        DockerStage {
            name: None,
            base_image: "debian:bookworm-slim".into(),
            workdir: "/app".into(),
            instructions: vec![
                "RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*".into(),
                format!("COPY --from=builder /app/target/release/{} .", binary_name),
            ],
            expose: Some(8080),
            cmd: Some(vec![format!("./{}", binary_name)]),
        },
    ];

    Ok(Some(SourceInfo {
        family: "Rust".into(),
        framework: Framework::Rust,
        version: None,
        port: 8080,
        env_vars: vec![],
        build_args: vec![],
        install_cmd: "cargo build --release".into(),
        build_cmd: Some("cargo build --release".into()),
        start_cmd: format!("./{}", binary_name),
        binary_name: Some(binary_name),
        entry_point: None,
        package_manager: Some("cargo".into()),
        has_lockfile: has_lock,
        dockerfile_stages: stages,
        dockerignore_entries: vec![
            "target".into(),
            ".git".into(),
            "*.md".into(),
            ".env*".into(),
            ".vscode".into(),
            ".idea".into(),
        ],
        notes: vec![],
    }))
}

fn detect_binary_name(cargo_toml: &str) -> String {
    // Look for [[bin]] name
    let mut in_bin = false;
    for line in cargo_toml.lines() {
        let line = line.trim();
        if line == "[[bin]]" {
            in_bin = true;
            continue;
        }
        if in_bin && line.starts_with("name") {
            if let Some(name) = extract_toml_string(line) {
                return name;
            }
        }
        if line.starts_with('[') && in_bin {
            in_bin = false;
        }
    }

    // Fallback: package.name
    let mut in_package = false;
    for line in cargo_toml.lines() {
        let line = line.trim();
        if line == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && line.starts_with("name") {
            if let Some(name) = extract_toml_string(line) {
                return name;
            }
        }
        if line.starts_with('[') && in_package {
            in_package = false;
        }
    }

    "app".to_string()
}

fn extract_toml_string(line: &str) -> Option<String> {
    // name = "my-app"
    let val = line.split('=').nth(1)?.trim();
    let val = val.trim_matches('"').trim_matches('\'');
    Some(val.to_string())
}
