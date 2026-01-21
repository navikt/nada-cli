mod jita;
mod kubeconfig;

use clap::{Parser, Subcommand};

/// Nada command-line interface.
#[derive(Debug, Clone, Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Subcommand)]
enum Commands {
    /// Gain just-in-time access for Google Cloud privileged resources.
    #[command(subcommand)]
    Jita(jita::Commands),
    /// Work with Kubernetes configuration.
    #[command(subcommand)]
    Kubeconfig(kubeconfig::Commands),
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("{0}")]
    Jita(#[from] jita::Error),
    #[error("{0}")]
    Kubeconfig(#[from] kubeconfig::Error),
}

async fn run_subcommand(command: Commands) -> Result<(), Error> {
    match command {
        Commands::Jita(subcommand) => match subcommand {
            jita::Commands::Entitlements => jita::entitlements().await?,
            jita::Commands::List => jita::grants().await?,
            jita::Commands::Grant {
                entitlement,
                duration,
                reason,
            } => {
                jita::grant_using_dialog(entitlement, duration.map(|d| d.as_secs()), reason).await?
            }
        },
        Commands::Kubeconfig(subcommand) => match subcommand {
            kubeconfig::Commands::Update => kubeconfig::update_config_file().await?,
        },
    };
    Ok(())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let Err(err) = run_subcommand(cli.command).await else {
        return;
    };
    println!("Command failed: {err}");
    std::process::exit(1);
}
