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
    Jita,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Jita => jita::run().unwrap(),
    }
}

pub mod jita {
    #[derive(Clone, Debug)]
    pub enum Error {}

    pub fn run() -> Result<(), Error> {
        Ok(())
    }
}
