use clap::{Parser, Subcommand};
use anyhow::Result;
use colored::Colorize;

mod api;
mod commands;
mod config;
mod ssh;
mod types;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Register a new user account
    Register,
    
    /// Log in to the ops.autos service
    Login,
    
    /// Display the current logged-in user info
    Whoami,
    
    /// Bind this server to a project environment
    Set {
        #[arg(long)]
        project: String,
        #[arg(long)]
        environment: String,
    },

    /// Manage projects
    #[command(subcommand)]
    Project(ProjectCommands),
    
    /// Interact with the current server environment
    #[command(subcommand)]
    Server(ServerCommands),

    /// Manage CI/CD keys for a project environment
    #[command(subcommand)]
    CiKey(CiKeyCommands),
}

#[derive(Subcommand)]
enum ProjectCommands {
    /// Create a new project
    Create { name: String },
}

#[derive(Subcommand)]
enum ServerCommands {
    /// Show information about the current server based on its IP
    Whoami,
}

#[derive(Subcommand)]
enum CiKeyCommands {
    /// Get the private SSH key for CI/CD deployment
    GetPrivate {
        #[arg(long)]
        project: String,
        #[arg(long)]
        environment: String,
    }
}


#[tokio::main]
async fn main() -> Result<()> {
    // --- FIX HERE: Change `.` to `::` ---
    let cli = Cli::parse();
    
    let result = match &cli.command {
        Commands::Register => commands::register::handle_register().await,
        Commands::Login => commands::login::handle_login().await,
        Commands::Whoami => commands::whoami::handle_whoami().await,
        Commands::Set { project, environment } => commands::set::handle_set(project.clone(), environment.clone()).await,
        Commands::Project(cmd) => match cmd {
            ProjectCommands::Create { name } => commands::project::handle_create_project(name.clone()).await,
        },
        Commands::Server(cmd) => match cmd {
            ServerCommands::Whoami => commands::server::handle_server_whoami().await,
        },
        Commands::CiKey(cmd) => match cmd {
            CiKeyCommands::GetPrivate { project, environment } => commands::ci_key::handle_get_ci_private_key(project.clone(), environment.clone()).await,
        },
    };

    if let Err(e) = result {
        eprintln!("{}: {}", "Error".red().bold(), e);
        std::process::exit(1);
    }
    
    Ok(())
}