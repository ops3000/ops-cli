use super::{DockerStage, Framework, SourceInfo};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn scan(dir: &Path) -> Result<Option<SourceInfo>> {
    let go_mod_path = dir.join("go.mod");
    if !go_mod_path.exists() {
        return Ok(None);
    }

    let go_mod = fs::read_to_string(&go_mod_path)?;
    let go_version = detect_go_version(&go_mod);
    let module_name = detect_module_name(&go_mod);
    let binary_name = module_name
        .rsplit('/')
        .next()
        .unwrap_or("server")
        .to_string();

    let has_go_sum = dir.join("go.sum").exists();
    let builder_base = format!("golang:{}-alpine", go_version);

    let stages = vec![
        DockerStage {
            name: Some("builder".into()),
            base_image: builder_base,
            workdir: "/app".into(),
            instructions: vec![
                format!("COPY go.mod go.sum{} ./", if has_go_sum { "" } else { "*" }),
                "RUN go mod download".into(),
                "COPY . .".into(),
                format!("RUN CGO_ENABLED=0 go build -o {} .", binary_name),
            ],
            expose: None,
            cmd: None,
        },
        DockerStage {
            name: None,
            base_image: "alpine:3.20".into(),
            workdir: "/app".into(),
            instructions: vec![
                "RUN apk add --no-cache ca-certificates".into(),
                format!("COPY --from=builder /app/{} .", binary_name),
            ],
            expose: Some(8080),
            cmd: Some(vec![format!("./{}", binary_name)]),
        },
    ];

    Ok(Some(SourceInfo {
        family: "Go".into(),
        framework: Framework::Go,
        version: Some(go_version),
        port: 8080,
        env_vars: vec![],
        build_args: vec![],
        install_cmd: "go mod download".into(),
        build_cmd: Some(format!("CGO_ENABLED=0 go build -o {} .", binary_name)),
        start_cmd: format!("./{}", binary_name),
        binary_name: Some(binary_name),
        entry_point: None,
        package_manager: None,
        has_lockfile: has_go_sum,
        dockerfile_stages: stages,
        dockerignore_entries: vec![
            ".git".into(),
            "*.md".into(),
            ".env*".into(),
            ".vscode".into(),
            ".idea".into(),
        ],
        notes: vec![],
    }))
}

fn detect_go_version(go_mod: &str) -> String {
    for line in go_mod.lines() {
        let line = line.trim();
        if line.starts_with("go ") {
            let ver = line.trim_start_matches("go ").trim();
            // "1.22.1" → "1.22"
            let parts: Vec<&str> = ver.split('.').collect();
            if parts.len() >= 2 {
                return format!("{}.{}", parts[0], parts[1]);
            }
            return ver.to_string();
        }
    }
    "1.22".to_string()
}

fn detect_module_name(go_mod: &str) -> String {
    for line in go_mod.lines() {
        let line = line.trim();
        if line.starts_with("module ") {
            return line.trim_start_matches("module ").trim().to_string();
        }
    }
    "app".to_string()
}
