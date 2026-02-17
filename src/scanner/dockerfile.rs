use super::SourceInfo;

/// Render a Dockerfile from SourceInfo stages
pub fn render_dockerfile(info: &SourceInfo) -> String {
    let mut out = String::new();

    for (i, stage) in info.dockerfile_stages.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }

        // FROM line
        if let Some(ref name) = stage.name {
            out.push_str(&format!("FROM {} AS {}\n", stage.base_image, name));
        } else {
            out.push_str(&format!("FROM {}\n", stage.base_image));
        }

        // WORKDIR
        out.push_str(&format!("WORKDIR {}\n", stage.workdir));

        // Instructions
        for instr in &stage.instructions {
            out.push_str(&format!("{}\n", instr));
        }

        // EXPOSE
        if let Some(port) = stage.expose {
            out.push_str(&format!("EXPOSE {}\n", port));
        }

        // CMD
        if let Some(ref cmd) = stage.cmd {
            let parts: Vec<String> = cmd.iter().map(|s| format!("\"{}\"", s)).collect();
            out.push_str(&format!("CMD [{}]\n", parts.join(", ")));
        }
    }

    out
}

/// Render a docker-compose.yml from project name + SourceInfo
pub fn render_compose(project_name: &str, info: &SourceInfo) -> String {
    let service_name = project_name
        .to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>();

    let mut out = String::new();
    out.push_str("services:\n");
    out.push_str(&format!("  {}:\n", service_name));
    out.push_str("    build: .\n");
    out.push_str(&format!("    ports:\n      - \"{}:{}\"\n", info.port, info.port));

    // Environment variables
    if !info.env_vars.is_empty() {
        out.push_str("    environment:\n");
        for (key, val) in &info.env_vars {
            out.push_str(&format!("      - {}={}\n", key, val));
        }
    }

    out.push_str("    restart: unless-stopped\n");

    out
}

/// Render .dockerignore from SourceInfo
pub fn render_dockerignore(info: &SourceInfo) -> String {
    let mut entries = info.dockerignore_entries.clone();
    // Always add common entries
    for e in &["Dockerfile", "docker-compose*.yml", ".dockerignore"] {
        let s = e.to_string();
        if !entries.contains(&s) {
            entries.push(s);
        }
    }
    entries.join("\n") + "\n"
}
