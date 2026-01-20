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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Jita => match jita::run().await {
            Ok(_) => {}
            Err(err) => println!("JITA error: {err}"),
        },
    }
}

pub mod jita {
    use google_cloud_gax::paginator::ItemPaginator;
    use google_cloud_privilegedaccessmanager_v1::client::PrivilegedAccessManager;

    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        #[error("Init: {0}")]
        Builder(#[from] google_cloud_gax::client_builder::Error),
        #[error("Request: {0}")]
        Request(#[from] google_cloud_privilegedaccessmanager_v1::Error),
    }

    pub async fn run() -> Result<(), Error> {
        println!("List entitlements");
        let client = PrivilegedAccessManager::builder().build().await?;
        let mut item_iterator = client
            .list_entitlements()
            .set_parent("folders/739335424622/locations/global")
            .by_item();
        while let Some(item) = item_iterator.next().await {
            let item = item?;
            println!("Entitlement: {}", item.name);
        }
        Ok(())
    }
}
