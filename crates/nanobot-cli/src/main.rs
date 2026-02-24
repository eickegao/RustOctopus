use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

mod cmd_agent;
mod cmd_cron;
mod cmd_gateway;
mod cmd_onboard;
mod cmd_status;

#[derive(Parser)]
#[command(name = "nanobot", about = "nanobot — Personal AI Assistant")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the agent (interactive or single-message mode)
    Agent {
        /// Send a single message and exit
        #[arg(short, long)]
        message: Option<String>,

        /// Session ID (default: "cli:default")
        #[arg(short, long, default_value = "cli:default")]
        session: String,
    },

    /// Start the full gateway server (agent + channels + cron)
    Gateway,

    /// Set up nanobot config and workspace
    Onboard,

    /// Show current configuration status
    Status,

    /// Manage scheduled cron jobs
    Cron {
        #[command(subcommand)]
        action: cmd_cron::CronAction,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Agent { message, session } => {
            let config = nanobot_core::config::loader::load_config(None)?;
            if let Some(msg) = message {
                cmd_agent::run_single(&msg, &session, config).await?;
            } else {
                cmd_agent::run_interactive(&session, config).await?;
            }
        }
        Commands::Gateway => {
            let config = nanobot_core::config::loader::load_config(None)?;
            cmd_gateway::run(config).await?;
        }
        Commands::Onboard => {
            cmd_onboard::run()?;
        }
        Commands::Status => {
            cmd_status::run()?;
        }
        Commands::Cron { action } => {
            cmd_cron::run(action)?;
        }
    }

    Ok(())
}
