use crate::nais_postgres;
use crate::nais_postgres::PostgresProxy;
use clap::Subcommand;
use tracing::log::{info, warn};

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
    #[error("Postgres: {0}")]
    Postgres(#[from] sqlx::Error),
}

/// Bootstrap a Nais Postgres database for use with Ingesterator.
///
/// FIXME: stub
pub async fn connect_and_enable_ingest(database: impl AsRef<str>) -> Result<(), Error> {
    info!("Opening proxy connection to Nais Postgres...");
    let connection = PostgresProxy::new(database).await?;
    info!("Connected to Nais Postgres!");

    let replication_config = ReplicationConfig {
        username: "cdc_replication".to_string(),
        password: "strong_password".to_string(),
        database: "ingesterator".to_string(),
        schema: "public".to_string(),
        tables: vec!["foo".to_string(), "bar".to_string()],
    };
    let pool = connection.pool();
    let mut source_db = SourceDatabase::new(pool, &replication_config);
    source_db.bootstrap_database_for_replication().await?;

    connection.close().await?;
    info!("Connection closed successfully.");

    Ok(())
}

use sqlx::{AssertSqlSafe, Pool, Postgres};

#[derive(Debug, Clone)]
pub struct ReplicationConfig {
    pub username: String,
    pub password: String,
    pub database: String,
    pub schema: String,
    pub tables: Vec<String>,
}

/// A connection to a source database where CDC should be set up.
pub struct SourceDatabase<'a, 'b> {
    pool: &'a Pool<Postgres>,
    config: &'b ReplicationConfig,
}

impl<'a, 'b> SourceDatabase<'a, 'b> {
    pub fn new(pool: &'a Pool<Postgres>, config: &'b ReplicationConfig) -> Self {
        Self { pool, config }
    }

    pub async fn bootstrap_database_for_replication(&mut self) -> Result<(), sqlx::Error> {
        // At this point, logical replication should already have been configured as such:

        // sqlx::query!("ALTER SYSTEM SET wal_level = 'logical';")
        // sqlx::query!("ALTER SYSTEM SET max_replication_slots = 4;")
        // sqlx::query!("ALTER SYSTEM SET max_wal_senders = 4;")
        // sqlx::query!("ALTER SYSTEM SET max_slot_wal_keep_size = '10GB';")

        if self.cdc_user_exists().await? {
            warn!("Drop existing CDC replication role and privileges");
            // fixme: replication roles should be set up using CRD
            self.drop_replication_role().await?;
        }

        // fixme: replication roles should be set up using CRD
        self.create_replication_role().await?;
        info!("Replication role configured");

        self.create_heartbeat_table().await?;
        info!("Heartbeat table configured");

        Ok(())
    }

    /// Returns true if the CDC user exists in the database.
    async fn cdc_user_exists(&self) -> Result<bool, sqlx::Error> {
        Ok(sqlx::query!(
            "SELECT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = $1);",
            &self.config.username
        )
        .fetch_one(self.pool)
        .await?
        .exists
        .unwrap_or_default())
    }

    /// Uninstall the CDC replication user by dropping all their privileges and deleting the role.
    async fn drop_replication_role(&mut self) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        let sql = AssertSqlSafe(format!(
            "REVOKE ALL PRIVILEGES ON ALL TABLES IN SCHEMA debezium FROM {};",
            &self.config.username
        ));
        sqlx::query(sql).execute(&mut *tx).await?;

        let sql = AssertSqlSafe(format!(
            "REVOKE ALL PRIVILEGES ON ALL TABLES IN SCHEMA {} FROM {};",
            &self.config.schema, &self.config.username
        ));
        sqlx::query(sql).execute(&mut *tx).await?;

        // Drop any previous privileges on the database.
        let sql = AssertSqlSafe(format!(
            "REVOKE ALL PRIVILEGES ON DATABASE {} FROM {};",
            &self.config.database, &self.config.username
        ));
        sqlx::query(sql).execute(&mut *tx).await?;

        // Drop the heartbeat table
        sqlx::query("DROP TABLE IF EXISTS debezium.heartbeat;")
            .execute(&mut *tx)
            .await?;

        // Drop the debezium schema
        sqlx::query("DROP SCHEMA debezium;")
            .execute(&mut *tx)
            .await?;

        // Finally, drop the CDC replication role if it exists.
        let sql = AssertSqlSafe(format!("DROP ROLE {};", &self.config.username));
        sqlx::query(sql).execute(&mut *tx).await?;

        tx.commit().await
    }

    /// Create a replication user and grant privileges.
    ///
    /// Note that parametrized values are not supported for these kinds of queries,
    /// so we do manual SQL string injection here.
    async fn create_replication_role(&self) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        // Use dedicated user with minimal privileges.
        let sql = AssertSqlSafe(format!(
            "CREATE ROLE {} WITH REPLICATION LOGIN PASSWORD '{}';",
            self.config.username, self.config.password
        ));
        sqlx::query(sql).execute(&mut *tx).await?;

        // Create replication user with minimal privileges
        let sql = AssertSqlSafe(format!(
            "GRANT CONNECT ON DATABASE {} TO {};",
            self.config.database, self.config.username,
        ));
        sqlx::query(sql).execute(&mut *tx).await?;

        // Grant replication permissions on tables
        for table in &self.config.tables {
            let sql = AssertSqlSafe(format!(
                "GRANT SELECT ON {} TO {};",
                table, self.config.username
            ));
            sqlx::query(sql).execute(&mut *tx).await?;
        }

        tx.commit().await
    }

    async fn create_heartbeat_table(&self) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        sqlx::query!("CREATE SCHEMA debezium;")
            .execute(&mut *tx)
            .await?;

        sqlx::query!("CREATE TABLE debezium.heartbeat (timestamp TIMESTAMPTZ);")
            .execute(&mut *tx)
            .await?;

        sqlx::query("INSERT INTO debezium.heartbeat VALUES (NOW());")
            .execute(&mut *tx)
            .await?;

        let sql = AssertSqlSafe(format!(
            "ALTER TABLE debezium.heartbeat OWNER TO {}",
            self.config.username
        ));
        sqlx::query(sql).execute(&mut *tx).await?;

        // let sql = AssertSqlSafe(format!(
        //     "GRANT ALL ON debezium.heartbeat TO {}",
        //     self.config.username
        // ));
        // sqlx::query(sql).execute(&mut *tx).await?;

        tx.commit().await
    }
}
