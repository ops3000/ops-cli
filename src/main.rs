use clap::{Parser, Subcommand};
use anyhow::Result;
use colored::Colorize;

mod api;
mod commands;
mod config;
mod serve;
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
    Logout,
    Whoami,

    /// Initialize this server as a node in OPS
    Init {
        /// Start ops serve daemon (default: true)
        #[arg(long, default_value = "true")]
        daemon: bool,
        /// Limit to specific projects (comma-separated)
        #[arg(long)]
        project: Option<String>,
        /// Limit to specific apps (comma-separated)
        #[arg(long)]
        app: Option<String>,
        /// Region for multi-region support (e.g., us-east, eu-west)
        #[arg(long)]
        region: Option<String>,
        /// ops serve port (default: 8377)
        #[arg(long, default_value = "8377")]
        port: u16,
        /// Custom hostname for this node
        #[arg(long)]
        hostname: Option<String>,
        /// Docker Compose project directory for ops serve
        #[arg(long)]
        compose_dir: Option<String>,
    },

    /// Manage nodes
    #[command(subcommand)]
    Node(NodeCommands),

    /// Bind this server (format: app.project or use --node for remote)
    Set {
        target: String,
        /// Node ID to bind (for remote binding)
        #[arg(long)]
        node: Option<u64>,
        /// Set as primary node
        #[arg(long)]
        primary: bool,
        /// Region for multi-region support (e.g., us-east, eu-west, ap-northeast)
        #[arg(long)]
        region: Option<String>,
        /// Availability zone (e.g., a, b, c)
        #[arg(long)]
        zone: Option<String>,
        /// Custom hostname for this node
        #[arg(long)]
        hostname: Option<String>,
        /// Load balancing weight (1-100)
        #[arg(long)]
        weight: Option<u8>,
    },

    /// SSH into a server or execute a command (format: environment.project)
    Ssh {
        target: String,
        /// (Optional) Command to execute on the remote server
        command: Option<String>,
    },

    /// Push a file or directory to the server (format: source environment.project[:/remote/path])
    Push {
        source: String,
        target: String,
    },

    /// Print the current session token to stdout
    Token,
    
    /// Manage environment variables for a target
    #[command(subcommand)]
    Env(EnvCommands),

    /// Manage projects
    #[command(subcommand)]
    Project(ProjectCommands),

    /// Interact with the current server environment
    #[command(subcommand)]
    Server(ServerCommands),

    /// Manage node groups for multi-region deployment
    #[command(subcommand)]
    NodeGroup(NodeGroupCommands),

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

    /// Deploy services defined in ops.toml
    Deploy {
        /// Path to ops.toml config file
        #[arg(short, long, default_value = "ops.toml")]
        file: String,
        /// Deploy only a specific service
        #[arg(long)]
        service: Option<String>,
        /// Skip build, only restart containers
        #[arg(long)]
        restart_only: bool,
    },

    /// Show status of deployed services (reads ops.toml)
    Status {
        /// Path to ops.toml
        #[arg(short, long, default_value = "ops.toml")]
        file: String,
    },

    /// View logs of a deployed service (reads ops.toml)
    Logs {
        /// Service name (e.g. jug0, juglans-api)
        service: String,
        /// Path to ops.toml
        #[arg(long, default_value = "ops.toml")]
        file: String,
        /// Number of lines to show
        #[arg(short = 'n', long, default_value = "100")]
        tail: u32,
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
    },

    /// Start HTTP server exposing container status, logs, metrics
    Serve {
        /// Bearer token for authentication
        #[arg(long)]
        token: String,
        /// Port to listen on
        #[arg(long, default_value = "8377")]
        port: u16,
        /// Docker Compose project directory
        #[arg(long)]
        compose_dir: String,
        /// Install as systemd service and configure nginx reverse proxy
        #[arg(long)]
        install: bool,
        /// Domain for nginx reverse proxy (e.g. api.RedQ.ops.autos)
        #[arg(long)]
        domain: Option<String>,
    },

    /// Update ops to the latest version
    Update,

    /// Check current version info
    Version,
}

#[derive(Subcommand)]
enum EnvCommands {
    /// Upload local .env file to the target server
    Upload { target: String },
    /// Download .env file from the target server
    Download { target: String },
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

#[derive(Subcommand)]
enum NodeCommands {
    /// List all your nodes
    List,
    /// Show detailed information about a node
    Info {
        /// Node ID
        id: u64,
    },
    /// Remove a node
    Remove {
        /// Node ID
        id: u64,
        /// Force deletion without confirmation
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum NodeGroupCommands {
    /// Create a new node group
    Create {
        /// Project name
        #[arg(short, long)]
        project: String,
        /// Environment name (e.g., prod, staging)
        #[arg(short, long)]
        env: String,
        /// Optional custom name for the group
        #[arg(long)]
        name: Option<String>,
        /// Load balancing strategy (round-robin, geo, weighted, failover)
        #[arg(long, default_value = "round-robin")]
        strategy: String,
    },
    /// List node groups for a project
    List {
        /// Project name (optional, lists all if not specified)
        #[arg(short, long)]
        project: Option<String>,
    },
    /// Show node group details including member nodes
    Show {
        /// Node group ID
        id: i64,
    },
    /// List nodes in a specific environment
    Nodes {
        /// Target in format: environment.project
        target: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Auto-update check (skip for certain commands)
    if !matches!(
        &cli.command,
        Commands::Update | Commands::Version | Commands::Serve { .. }
    ) {
        if let Ok(true) = update::check_and_auto_update() {
            return Ok(()); // Exit after update, user should re-run
        }
    }

    let result = match &cli.command {
        Commands::Register => commands::register::handle_register().await,
        Commands::Login => commands::login::handle_login().await,
        Commands::Logout => commands::logout::handle_logout().await,
        Commands::Whoami => commands::whoami::handle_whoami().await,

        Commands::Init { daemon, project, app, region, port, hostname, compose_dir } =>
            commands::init::handle_init(
                *daemon,
                project.clone(),
                app.clone(),
                region.clone(),
                *port,
                hostname.clone(),
                compose_dir.clone(),
            ).await,

        Commands::Node(cmd) => match cmd {
            NodeCommands::List => commands::node::handle_list().await,
            NodeCommands::Info { id } => commands::node::handle_info(*id).await,
            NodeCommands::Remove { id, force } => commands::node::handle_remove(*id, *force).await,
        },

        Commands::Set { target, node, primary, region, zone, hostname, weight } =>
            commands::set::handle_set(target.clone(), *node, *primary, region.clone(), zone.clone(), hostname.clone(), *weight).await,
        Commands::Ssh { target, command } => commands::ssh::handle_ssh(target.clone(), command.clone()).await,
        Commands::Push { source, target } => commands::scp::handle_push(source.clone(), target.clone()).await,

        Commands::Token => commands::token::handle_get_token().await,

        Commands::Env(cmd) => match cmd {
            EnvCommands::Upload { target } => commands::env::handle_upload(target.clone()).await,
            EnvCommands::Download { target } => commands::env::handle_download(target.clone()).await,
        },

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
        Commands::NodeGroup(cmd) => match cmd {
            NodeGroupCommands::Create { project, env, name, strategy } =>
                commands::node_group::handle_create(project.clone(), env.clone(), name.clone(), strategy.clone()).await,
            NodeGroupCommands::List { project } =>
                commands::node_group::handle_list(project.clone()).await,
            NodeGroupCommands::Show { id } =>
                commands::node_group::handle_show(*id).await,
            NodeGroupCommands::Nodes { target } =>
                commands::node_group::handle_nodes(target.clone()).await,
        },
        
        Commands::Deploy { file, service, restart_only } =>
            commands::deploy::handle_deploy(file.clone(), service.clone(), *restart_only).await,
        Commands::Status { file } =>
            commands::status::handle_status(file.clone()).await,
        Commands::Logs { service, file, tail, follow } =>
            commands::logs::handle_logs(file.clone(), service.clone(), *tail, *follow).await,

        Commands::Serve { token, port, compose_dir, install, domain } => {
            if *install {
                commands::serve::handle_install(token.clone(), *port, compose_dir.clone(), domain.clone()).await
            } else {
                commands::serve::handle_serve(token.clone(), *port, compose_dir.clone()).await
            }
        },

        Commands::Update => commands::update::handle_update().await,
        Commands::Version => {
            println!("ops-cli version: {}", env!("CARGO_PKG_VERSION").cyan());
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