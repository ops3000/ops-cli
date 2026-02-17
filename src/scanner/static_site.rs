use super::{DockerStage, Framework, SourceInfo};
use anyhow::Result;
use std::path::Path;

pub fn scan(dir: &Path) -> Result<Option<SourceInfo>> {
    if !dir.join("index.html").exists() {
        return Ok(None);
    }

    // Don't match if there's a recognized language marker
    if dir.join("package.json").exists()
        || dir.join("Cargo.toml").exists()
        || dir.join("go.mod").exists()
        || dir.join("requirements.txt").exists()
        || dir.join("pyproject.toml").exists()
    {
        return Ok(None);
    }

    let stages = vec![
        DockerStage {
            name: None,
            base_image: "nginx:alpine".into(),
            workdir: "/usr/share/nginx/html".into(),
            instructions: vec![
                "COPY . .".into(),
            ],
            expose: Some(80),
            cmd: Some(vec!["nginx".into(), "-g".into(), "daemon off;".into()]),
        },
    ];

    Ok(Some(SourceInfo {
        family: "Static Site".into(),
        framework: Framework::StaticSite,
        version: None,
        port: 80,
        env_vars: vec![],
        build_args: vec![],
        install_cmd: String::new(),
        build_cmd: None,
        start_cmd: "nginx -g 'daemon off;'".into(),
        binary_name: None,
        entry_point: None,
        package_manager: None,
        has_lockfile: false,
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
