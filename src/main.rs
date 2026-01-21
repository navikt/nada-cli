mod jita;

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
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let command_result = match cli.command {
        Commands::Jita(jita::Commands::Entitlements) => jita::entitlements().await,
        Commands::Jita(jita::Commands::List) => jita::grants().await,
        Commands::Jita(jita::Commands::Grant {
            entitlement,
            duration,
            reason,
        }) => jita::grant_using_dialog(entitlement, duration.map(|d| d.as_secs()), reason).await,
    };
    let Err(err) = command_result else {
        return;
    };
    println!("Command failed: {err}");
    std::process::exit(1);
}
