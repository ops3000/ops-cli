use super::{DockerStage, Framework, SourceInfo};
use anyhow::Result;
use std::fs;
use std::path::Path;

/// Detect Python version from .python-version or pyproject.toml
fn detect_python_version(dir: &Path) -> String {
    // .python-version
    if let Ok(v) = fs::read_to_string(dir.join(".python-version")) {
        let v = v.trim();
        if !v.is_empty() {
            // "3.12.1" → "3.12"
            let parts: Vec<&str> = v.split('.').collect();
            if parts.len() >= 2 {
                return format!("{}.{}", parts[0], parts[1]);
            }
            return v.to_string();
        }
    }
    // pyproject.toml — look for requires-python
    if let Ok(content) = fs::read_to_string(dir.join("pyproject.toml")) {
        for line in content.lines() {
            if line.contains("requires-python") {
                // requires-python = ">=3.12"
                let digits: String = line.chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                if !digits.is_empty() {
                    let parts: Vec<&str> = digits.split('.').collect();
                    if parts.len() >= 2 {
                        return format!("{}.{}", parts[0], parts[1]);
                    }
                    return digits;
                }
            }
        }
    }
    "3.12".to_string()
}

/// Read requirements.txt content (or empty)
fn read_requirements(dir: &Path) -> Option<String> {
    fs::read_to_string(dir.join("requirements.txt")).ok()
}

/// Check if a dependency exists in requirements.txt or pyproject.toml
fn has_python_dep(dir: &Path, name: &str) -> bool {
    if let Some(reqs) = read_requirements(dir) {
        if reqs.lines().any(|l| {
            let l = l.trim().to_lowercase();
            l.starts_with(&name.to_lowercase()) && (l.len() == name.len() || !l.as_bytes()[name.len()].is_ascii_alphanumeric())
        }) {
            return true;
        }
    }
    if let Ok(content) = fs::read_to_string(dir.join("pyproject.toml")) {
        if content.to_lowercase().contains(name) {
            return true;
        }
    }
    if let Ok(content) = fs::read_to_string(dir.join("Pipfile")) {
        if content.to_lowercase().contains(name) {
            return true;
        }
    }
    false
}

/// Determine install command based on package manager
fn detect_install_cmd(dir: &Path) -> (String, String) {
    if dir.join("pyproject.toml").exists() {
        let content = fs::read_to_string(dir.join("pyproject.toml")).unwrap_or_default();
        if content.contains("[tool.poetry]") {
            return ("poetry".into(), "pip install poetry && poetry install --no-dev".into());
        }
    }
    if dir.join("Pipfile").exists() {
        return ("pipenv".into(), "pip install pipenv && pipenv install --deploy --system".into());
    }
    if dir.join("requirements.txt").exists() {
        return ("pip".into(), "pip install --no-cache-dir -r requirements.txt".into());
    }
    if dir.join("pyproject.toml").exists() {
        return ("pip".into(), "pip install --no-cache-dir .".into());
    }
    ("pip".into(), "pip install --no-cache-dir -r requirements.txt".into())
}

fn python_dockerignore() -> Vec<String> {
    vec![
        "__pycache__".into(),
        "*.pyc".into(),
        ".venv".into(),
        "venv".into(),
        ".env*".into(),
        ".git".into(),
        "*.md".into(),
        ".vscode".into(),
        ".idea".into(),
        ".pytest_cache".into(),
        ".mypy_cache".into(),
    ]
}

// ─── Django ───────────────────────────────────────────────────────

pub fn scan_django(dir: &Path) -> Result<Option<SourceInfo>> {
    if !dir.join("manage.py").exists() {
        return Ok(None);
    }
    if !has_python_dep(dir, "django") {
        return Ok(None);
    }

    let py_ver = detect_python_version(dir);
    let base = format!("python:{}-slim", py_ver);
    let (pm, install_cmd) = detect_install_cmd(dir);

    // Try to detect WSGI module from manage.py or settings
    let wsgi_module = detect_django_wsgi(dir).unwrap_or_else(|| "myapp.wsgi:application".into());

    let stages = vec![
        DockerStage {
            name: None,
            base_image: base,
            workdir: "/app".into(),
            instructions: vec![
                copy_deps_instruction(&pm),
                format!("RUN {}", install_cmd),
                "COPY . .".into(),
                "RUN python manage.py collectstatic --noinput 2>/dev/null || true".into(),
            ],
            expose: Some(8000),
            cmd: Some(vec![
                "gunicorn".into(),
                wsgi_module.clone(),
                "--bind".into(),
                "0.0.0.0:8000".into(),
            ]),
        },
    ];

    let mut notes = vec![];
    if !has_python_dep(dir, "gunicorn") {
        notes.push("Add `gunicorn` to requirements.txt for production".into());
    }

    Ok(Some(SourceInfo {
        family: "Django".into(),
        framework: Framework::Django,
        version: Some(py_ver),
        port: 8000,
        env_vars: vec![],
        build_args: vec![],
        install_cmd,
        build_cmd: Some("python manage.py collectstatic --noinput".into()),
        start_cmd: format!("gunicorn {} --bind 0.0.0.0:8000", wsgi_module),
        binary_name: None,
        entry_point: Some(wsgi_module),
        package_manager: Some(pm),
        has_lockfile: false,
        dockerfile_stages: stages,
        dockerignore_entries: python_dockerignore(),
        notes,
    }))
}

fn detect_django_wsgi(dir: &Path) -> Option<String> {
    // Read manage.py to find DJANGO_SETTINGS_MODULE
    if let Ok(content) = fs::read_to_string(dir.join("manage.py")) {
        for line in content.lines() {
            if line.contains("DJANGO_SETTINGS_MODULE") {
                // os.environ.setdefault('DJANGO_SETTINGS_MODULE', 'myapp.settings')
                if let Some(start) = line.find('\'').or_else(|| line.find('"')) {
                    let rest = &line[start + 1..];
                    if let Some(mid) = rest.find('\'').or_else(|| rest.find('"')) {
                        let rest2 = &rest[mid + 1..];
                        // Skip the comma and whitespace, find next quote
                        if let Some(s2) = rest2.find('\'').or_else(|| rest2.find('"')) {
                            let rest3 = &rest2[s2 + 1..];
                            if let Some(e2) = rest3.find('\'').or_else(|| rest3.find('"')) {
                                let module = &rest3[..e2]; // e.g., "myapp.settings"
                                // Convert "myapp.settings" → "myapp.wsgi:application"
                                if let Some(dot) = module.rfind('.') {
                                    return Some(format!("{}.wsgi:application", &module[..dot]));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn copy_deps_instruction(pm: &str) -> String {
    match pm {
        "poetry" => "COPY pyproject.toml poetry.lock* ./".into(),
        "pipenv" => "COPY Pipfile Pipfile.lock* ./".into(),
        _ => "COPY requirements.txt ./".into(),
    }
}

// ─── Flask ────────────────────────────────────────────────────────

pub fn scan_flask(dir: &Path) -> Result<Option<SourceInfo>> {
    if !has_python_dep(dir, "flask") {
        return Ok(None);
    }

    let py_ver = detect_python_version(dir);
    let base = format!("python:{}-slim", py_ver);
    let (pm, install_cmd) = detect_install_cmd(dir);
    let entry = detect_flask_entry(dir);

    let stages = vec![
        DockerStage {
            name: None,
            base_image: base,
            workdir: "/app".into(),
            instructions: vec![
                copy_deps_instruction(&pm),
                format!("RUN {}", install_cmd),
                "COPY . .".into(),
            ],
            expose: Some(5000),
            cmd: Some(vec![
                "gunicorn".into(),
                entry.clone(),
                "--bind".into(),
                "0.0.0.0:5000".into(),
            ]),
        },
    ];

    let mut notes = vec![];
    if !has_python_dep(dir, "gunicorn") {
        notes.push("Add `gunicorn` to requirements.txt for production".into());
    }

    Ok(Some(SourceInfo {
        family: "Flask".into(),
        framework: Framework::Flask,
        version: Some(py_ver),
        port: 5000,
        env_vars: vec![],
        build_args: vec![],
        install_cmd,
        build_cmd: None,
        start_cmd: format!("gunicorn {} --bind 0.0.0.0:5000", entry),
        binary_name: None,
        entry_point: Some(entry),
        package_manager: Some(pm),
        has_lockfile: false,
        dockerfile_stages: stages,
        dockerignore_entries: python_dockerignore(),
        notes,
    }))
}

fn detect_flask_entry(dir: &Path) -> String {
    // Check common entry points
    for (file, module) in &[
        ("app.py", "app:app"),
        ("application.py", "application:app"),
        ("wsgi.py", "wsgi:app"),
        ("main.py", "main:app"),
    ] {
        if dir.join(file).exists() {
            return module.to_string();
        }
    }
    // Check for app/__init__.py
    if dir.join("app/__init__.py").exists() {
        return "app:app".into();
    }
    "app:app".into()
}

// ─── FastAPI ──────────────────────────────────────────────────────

pub fn scan_fastapi(dir: &Path) -> Result<Option<SourceInfo>> {
    if !has_python_dep(dir, "fastapi") {
        return Ok(None);
    }

    let py_ver = detect_python_version(dir);
    let base = format!("python:{}-slim", py_ver);
    let (pm, install_cmd) = detect_install_cmd(dir);
    let entry = detect_fastapi_entry(dir);

    let stages = vec![
        DockerStage {
            name: None,
            base_image: base,
            workdir: "/app".into(),
            instructions: vec![
                copy_deps_instruction(&pm),
                format!("RUN {}", install_cmd),
                "COPY . .".into(),
            ],
            expose: Some(8000),
            cmd: Some(vec![
                "uvicorn".into(),
                entry.clone(),
                "--host".into(),
                "0.0.0.0".into(),
                "--port".into(),
                "8000".into(),
            ]),
        },
    ];

    let mut notes = vec![];
    if !has_python_dep(dir, "uvicorn") {
        notes.push("Add `uvicorn` to requirements.txt for production".into());
    }

    Ok(Some(SourceInfo {
        family: "FastAPI".into(),
        framework: Framework::FastApi,
        version: Some(py_ver),
        port: 8000,
        env_vars: vec![],
        build_args: vec![],
        install_cmd,
        build_cmd: None,
        start_cmd: format!("uvicorn {} --host 0.0.0.0 --port 8000", entry),
        binary_name: None,
        entry_point: Some(entry),
        package_manager: Some(pm),
        has_lockfile: false,
        dockerfile_stages: stages,
        dockerignore_entries: python_dockerignore(),
        notes,
    }))
}

fn detect_fastapi_entry(dir: &Path) -> String {
    for (file, module) in &[
        ("main.py", "main:app"),
        ("app.py", "app:app"),
        ("app/main.py", "app.main:app"),
        ("src/main.py", "src.main:app"),
    ] {
        if dir.join(file).exists() {
            return module.to_string();
        }
    }
    "main:app".into()
}

// ─── Generic Python ───────────────────────────────────────────────

pub fn scan_generic(dir: &Path) -> Result<Option<SourceInfo>> {
    let has_reqs = dir.join("requirements.txt").exists();
    let has_pyproject = dir.join("pyproject.toml").exists();
    let has_pipfile = dir.join("Pipfile").exists();

    if !has_reqs && !has_pyproject && !has_pipfile {
        return Ok(None);
    }

    let py_ver = detect_python_version(dir);
    let base = format!("python:{}-slim", py_ver);
    let (pm, install_cmd) = detect_install_cmd(dir);

    // Try to guess start command
    let start_cmd: String = if dir.join("main.py").exists() {
        "python main.py".into()
    } else if dir.join("app.py").exists() {
        "python app.py".into()
    } else {
        "python main.py".into()
    };

    let stages = vec![
        DockerStage {
            name: None,
            base_image: base,
            workdir: "/app".into(),
            instructions: vec![
                copy_deps_instruction(&pm),
                format!("RUN {}", install_cmd),
                "COPY . .".into(),
            ],
            expose: Some(8000),
            cmd: Some(start_cmd.split_whitespace().map(String::from).collect()),
        },
    ];

    Ok(Some(SourceInfo {
        family: "Python".into(),
        framework: Framework::GenericPython,
        version: Some(py_ver),
        port: 8000,
        env_vars: vec![],
        build_args: vec![],
        install_cmd,
        build_cmd: None,
        start_cmd,
        binary_name: None,
        entry_point: None,
        package_manager: Some(pm),
        has_lockfile: false,
        dockerfile_stages: stages,
        dockerignore_entries: python_dockerignore(),
        notes: vec![],
    }))
}
