mod ingest;
mod jita;
mod kubeconfig;
mod nais_postgres;
#[allow(unused)]
mod postgres_crd;

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
    /// Configure Nais Postgres instances for ingestion.
    #[command(subcommand)]
    Ingest(ingest::Commands),
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("{0}")]
    Jita(#[from] jita::Error),
    #[error("{0}")]
    Kubeconfig(#[from] kubeconfig::Error),
    #[error("{0}")]
    Ingest(#[from] ingest::Error),
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
            kubeconfig::Commands::Update {
                folders,
                locations,
                gateway,
            } => kubeconfig::update_config_file(folders, locations, gateway).await?,
        },
        Commands::Ingest(subcommand) => match subcommand {
            ingest::Commands::Enable { database } => {
                ingest::connect_and_enable_ingest(&database).await?
            }
        },
    };
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let Err(err) = run_subcommand(cli.command).await else {
        return;
    };
    println!("Command failed: {err}");
    std::process::exit(1);
}
