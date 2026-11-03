use crate::nais_postgres;
use crate::nais_postgres::PostgresProxy;
use clap::Subcommand;
use tracing::log::info;

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// (preview) configure Nais Postgres for ingestion.
    Enable {
        /// Name of the database to configure.
        #[arg()]
        database: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Nais Postgres proxy: {0}")]
    Proxy(#[from] nais_postgres::Error),
}

/// Bootstrap a Nais Postgres database for use with Ingesterator.
///
/// FIXME: stub
pub async fn connect_and_enable_ingest(database: impl AsRef<str>) -> Result<(), Error> {
    info!("Opening proxy connection to Nais Postgres...");
    let connection = PostgresProxy::new(database).await?;
    
    info!("Connected to Nais Postgres!");
    
    connection.close().await?;
    info!("Connection closed successfully.");
    
    Ok(())
}
