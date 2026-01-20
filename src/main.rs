use clap::{Parser, Subcommand};

/// Nada's global GCP folder.
const GLOBAL_FOLDER: &'static str = "folders/739335424622/locations/global";

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
    match cli.command {
        Commands::Jita(jita::Commands::Entitlements) => match jita::entitlements().await {
            Ok(_) => {}
            Err(err) => println!("List entitlements: {err}"),
        },
        Commands::Jita(jita::Commands::List) => match jita::grants().await {
            Ok(_) => {}
            Err(err) => println!("List JITA grants: {err}"),
        },
        Commands::Jita(jita::Commands::Grant) => match jita::grant().await {
            Ok(_) => {}
            Err(err) => println!("Grant just-in-time access: {err}"),
        },
    }
}

pub mod jita {
    use crate::GLOBAL_FOLDER;
    use clap::Subcommand;
    use google_cloud_gax::paginator::ItemPaginator;
    use google_cloud_privilegedaccessmanager_v1::client::PrivilegedAccessManager;
    use google_cloud_privilegedaccessmanager_v1::model::{
        CreateGrantRequest, Entitlement, Grant, Justification,
    };
    use std::time::Duration;

    #[derive(Debug, Clone, Subcommand)]
    pub enum Commands {
        /// List possible privilege entitlements.
        Entitlements,
        /// List my elevated privileges.
        List,
        /// Grant elevated privileges to me.
        Grant,
    }

    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        #[error("Init: {0}")]
        Builder(#[from] google_cloud_gax::client_builder::Error),
        #[error("Request: {0}")]
        Request(#[from] google_cloud_privilegedaccessmanager_v1::Error),
    }

    async fn list_entitlements() -> Result<Vec<Entitlement>, Error> {
        let client = PrivilegedAccessManager::builder().build().await?;
        let mut result = vec![];
        let mut item_iterator = client
            .list_entitlements()
            .set_parent(GLOBAL_FOLDER)
            .by_item();
        while let Some(item) = item_iterator.next().await {
            result.push(item?);
        }
        Ok(result)
    }

    pub async fn entitlements() -> Result<(), Error> {
        println!("=== List of Nada entitlements ===");
        let entitlements = list_entitlements().await?;
        for ent in entitlements {
            let Some(friendly_name) = ent
                .name
                .strip_prefix(GLOBAL_FOLDER)
                .and_then(|unfriendly_name| unfriendly_name.strip_prefix("/entitlements/"))
            else {
                continue;
            };
            println!("- {friendly_name}");
        }
        Ok(())
    }

    pub async fn grants() -> Result<(), Error> {
        let entitlements = list_entitlements().await?;
        let client = PrivilegedAccessManager::builder().build().await?;
        for ent in entitlements {
            let mut item_iterator = client.list_grants().set_parent(&ent.name).by_item();
            while let Some(item) = item_iterator.next().await {
                let item = item?;
                let Some(friendly_name) = item.name.strip_prefix(&ent.name)
                //.and_then(|unfriendly_name| unfriendly_name.strip_prefix("/entitlements/"))
                else {
                    continue;
                };
                println!("- {friendly_name}");
            }
        }
        Ok(())
    }

    pub async fn grant() -> Result<(), Error> {
        let entitlements = list_entitlements().await?;
        create_grant(entitlements.first().unwrap()).await
    }

    pub async fn create_grant(entitlement: &Entitlement) -> Result<(), Error> {
        let client = PrivilegedAccessManager::builder().build().await?;
        let hour = google_cloud_wkt::Duration::new(3600, 0).unwrap();
        let justification = Justification::new().set_unstructured_justification("dette er en test");
        let grant = Grant::new()
            .set_requested_duration(hour)
            .set_justification(justification);
        let req = CreateGrantRequest::new().set_grant(grant);
        let grant = client
            .create_grant()
            .with_request(req)
            .set_parent(entitlement.name.clone())
            .send()
            .await?;
        println!("Grant: {grant:?}");
        Ok(())
    }
}
