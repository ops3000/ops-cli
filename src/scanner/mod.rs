pub mod node;
pub mod python;
pub mod gomod;
pub mod rust;
pub mod static_site;
pub mod dockerfile;

use anyhow::Result;
use std::path::Path;

/// Detected framework
#[derive(Debug, Clone, PartialEq)]
pub enum Framework {
    NextJs,
    NuxtJs,
    ViteSpa,
    RemixJs,
    NodeApi,
    GenericNode,
    FastApi,
    Django,
    Flask,
    GenericPython,
    Go,
    Rust,
    StaticSite,
}

impl Framework {
    pub fn display_name(&self) -> &'static str {
        match self {
            Framework::NextJs => "Next.js",
            Framework::NuxtJs => "Nuxt",
            Framework::ViteSpa => "Vite SPA",
            Framework::RemixJs => "Remix",
            Framework::NodeApi => "Node.js API",
            Framework::GenericNode => "Node.js",
            Framework::FastApi => "FastAPI",
            Framework::Django => "Django",
            Framework::Flask => "Flask",
            Framework::GenericPython => "Python",
            Framework::Go => "Go",
            Framework::Rust => "Rust",
            Framework::StaticSite => "Static Site",
        }
    }

    pub fn default_port(&self) -> u16 {
        match self {
            Framework::NextJs => 3000,
            Framework::NuxtJs => 3000,
            Framework::ViteSpa => 80,
            Framework::RemixJs => 3000,
            Framework::NodeApi | Framework::GenericNode => 3000,
            Framework::FastApi => 8000,
            Framework::Django => 8000,
            Framework::Flask => 5000,
            Framework::GenericPython => 8000,
            Framework::Go => 8080,
            Framework::Rust => 8080,
            Framework::StaticSite => 80,
        }
    }
}

/// A single stage in a multi-stage Dockerfile
#[derive(Debug, Clone)]
pub struct DockerStage {
    pub name: Option<String>,
    pub base_image: String,
    pub workdir: String,
    pub instructions: Vec<String>,
    pub expose: Option<u16>,
    pub cmd: Option<Vec<String>>,
}

/// Full project scan result — everything needed to generate Dockerfile + ops.toml
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub family: String,
    pub framework: Framework,
    pub version: Option<String>,
    pub port: u16,
    pub env_vars: Vec<(String, String)>,
    pub build_args: Vec<(String, String)>,
    pub install_cmd: String,
    pub build_cmd: Option<String>,
    pub start_cmd: String,
    pub binary_name: Option<String>,
    pub entry_point: Option<String>,
    pub package_manager: Option<String>,
    pub has_lockfile: bool,
    pub dockerfile_stages: Vec<DockerStage>,
    pub dockerignore_entries: Vec<String>,
    pub notes: Vec<String>,
}

type ScannerFn = fn(&Path) -> Result<Option<SourceInfo>>;

/// Ordered list of scanners — framework-specific first, then generic language, then fallback
fn scanners() -> Vec<(&'static str, ScannerFn)> {
    vec![
        // Framework-level (high priority)
        ("Next.js",    node::scan_nextjs),
        ("Nuxt",       node::scan_nuxtjs),
        ("Remix",      node::scan_remix),
        ("Vite SPA",   node::scan_vite_spa),
        ("Django",     python::scan_django),
        ("Flask",      python::scan_flask),
        ("FastAPI",    python::scan_fastapi),
        // Language-level
        ("Node.js",    node::scan_generic),
        ("Python",     python::scan_generic),
        ("Go",         gomod::scan),
        ("Rust",       rust::scan),
        // Fallback
        ("Static",     static_site::scan),
    ]
}

/// Run all scanners in priority order, return first match
pub fn scan(source_dir: &Path) -> Result<Option<SourceInfo>> {
    for (_name, scanner) in scanners() {
        if let Some(info) = scanner(source_dir)? {
            return Ok(Some(info));
        }
    }
    Ok(None)
}
