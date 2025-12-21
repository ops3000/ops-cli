use clap::{Parser, Subcommand};
use anyhow::Result;
use colored::Colorize;

mod api;
mod commands;
mod config;
mod ssh;
mod types;
mod utils;
mod update;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Register,
    Login,
    Whoami,
    
    /// Bind this server (format: environment.project)
    Set {
        target: String,
    },

    /// SSH into a server (format: environment.project)
    Ssh {
        target: String,
    },

    /// Manage projects
    #[command(subcommand)]
    Project(ProjectCommands),

    /// Interact with the current server environment
    #[command(subcommand)]
    Server(ServerCommands),

    #[command(alias = "ci-key")]
    CiKeys {
        target: String,
    },
    
    /// Get the public IP address of a server
    Ip {
        target: String,
    },

    /// Ping a server to check its reachability
    Ping {
        target: String,
    },

    /// Update ops to the latest version
    Update,
    
    /// Check current version info
    Version,
}

#[derive(Subcommand)]
enum ProjectCommands {
    /// Create a new project
    Create { name: String },

    /// List all projects and their servers. Optional name to filter.
    List {
        name: Option<String>,
    },
}

#[derive(Subcommand)]
enum ServerCommands {
    /// Show information about the current server based on its IP
    Whoami,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // --- 移除：删除了自动检查更新的逻辑，防止卡顿 ---

    let result = match &cli.command {
        Commands::Register => commands::register::handle_register().await,
        Commands::Login => commands::login::handle_login().await,
        Commands::Whoami => commands::whoami::handle_whoami().await,
        
        Commands::Set { target } => commands::set::handle_set(target.clone()).await,
        Commands::Ssh { target } => commands::ssh::handle_ssh(target.clone()).await,
        Commands::CiKeys { target } => commands::ci_key::handle_get_ci_private_key(target.clone()).await,

        Commands::Ip { target } => commands::ip::handle_ip(target.clone()).await,
        Commands::Ping { target } => commands::ping::handle_ping(target.clone()).await,

        Commands::Project(cmd) => match cmd {
            ProjectCommands::Create { name } => commands::project::handle_create_project(name.clone()).await,
            ProjectCommands::List { name } => commands::project::handle_list_projects(name.clone()).await,
        },
        Commands::Server(cmd) => match cmd {
            ServerCommands::Whoami => commands::server::handle_server_whoami().await,
        },
        
        Commands::Update => commands::update::handle_update().await,
        Commands::Version => {
            println!("ops-cli version: {}", env!("CARGO_PKG_VERSION").cyan());
            // Version 命令仍然保留手动检查功能，如果用户主动运行 ops version
            tokio::task::spawn_blocking(|| {
                if let Ok(Some(v)) = update::check_for_update(false) {
                    println!("Latest version:  {}", v.green());
                } else {
                    println!("You are on the latest version.");
                }
            }).await?;
            Ok(())
        },
    };

    if let Err(e) = result {
        eprintln!("{}: {}", "Error".red().bold(), e);
        std::process::exit(1);
    }
    
    Ok(())
}