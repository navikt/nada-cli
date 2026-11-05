use serde::Deserialize;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::{Pool, Postgres};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::Duration;

/// A proxied connection to a Nais Postgres instance.
///
/// The proxy is initialized by running `nais postgres proxy`.
///
/// The Nais command-line interface will keep the connection open indefinitely.
/// To drop the proxied connection, callers should call `close()` after finishing.
/// Otherwise, both the Postgres and proxy connections will linger until the program finishes.
pub struct PostgresProxy {
    nais_cli_process: std::process::Child,
    pool: Pool<Postgres>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unable to capture standard output")]
    Stdout,
    #[error("got invalid port: {0}")]
    InvalidPort(String),
    #[error("got invalid duration: {0}")]
    InvalidTimeout(String),
    #[error("got invalid SSL mode: {0}")]
    InvalidSSLMode(String),
    #[error("OS: {0}")]
    OS(#[from] std::io::Error),
    #[error("Postgres: {0}")]
    Postgres(#[from] sqlx::Error),
    #[error("deserialize: {0}")]
    Deserialize(#[from] serde_json::Error),
}

impl PostgresProxy {
    /// Reason for Postgres privilege escalation.
    /// This reason might be used in a official court.
    const NAIS_CLI_JITA_REASON: &'static str = "Configuring database for warehouse ingestion";

    /// Use the Nais command-line interface to set up a connection proxy to a Nais Postgres instance.
    pub async fn new(database: impl AsRef<str>) -> Result<Self, Error> {
        let mut command = Command::new("nais");

        command
            .arg("postgres")
            .arg("proxy")
            .arg("--print-password")
            .arg("--exit-on-stdin-eof")
            .arg("--access-level")
            .arg("admin")
            .arg("--output")
            .arg("json")
            .arg("--reason")
            .arg(Self::NAIS_CLI_JITA_REASON)
            .arg(database.as_ref());

        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::null());

        let mut nais_cli_process = command.spawn()?;

        let stdout = nais_cli_process.stdout.take().ok_or(Error::Stdout)?;
        // let lines = BufReader::new(stdout).lines();
        let output: NaisPostgresProxyOutput = serde_json::from_reader(stdout)?;
        let postgres_params: PostgresParams = output.try_into()?;

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(postgres_params.timeout)
            .connect_with(postgres_params.options)
            .await?;

        Ok(Self {
            pool,
            nais_cli_process,
        })
    }

    /// Return a reference to the Postgres connection pool.
    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }

    /// Close the Postgres connection,
    /// then kill the proxy and wait until the process is dead.
    pub async fn close(mut self) -> Result<(), Error> {
        self.pool.close().await;
        self.nais_cli_process.wait()?;
        Ok(())
    }
}

/// Nais CLI's JSON output.
#[derive(Deserialize)]
#[serde(rename_all = "UPPERCASE")]
struct NaisPostgresProxyOutput {
    pgconnect_timeout: String,
    pgdatabase: String,
    pghostaddr: String,
    pgport: String,
    pgsslmode: String,
    pgsslrootcert: String,
    pguser: String,
    pgpassword: String,
    #[allow(unused)]
    pghost: String,
}

struct PostgresParams {
    options: PgConnectOptions,
    timeout: Duration,
}

impl TryInto<PostgresParams> for NaisPostgresProxyOutput {
    type Error = Error;

    /// Parse Nais CLI's Postgres JSON output to a configuration
    /// struct that can be used to configure `sqlx`.
    fn try_into(self) -> Result<PostgresParams, Self::Error> {
        let port = self
            .pgport
            .parse()
            .map_err(|_| Error::InvalidPort(self.pgport.to_string()))?;

        let timeout_secs = self
            .pgconnect_timeout
            .parse()
            .map_err(|_| Error::InvalidTimeout(self.pgconnect_timeout.to_string()))?;

        let timeout = Duration::from_secs(timeout_secs);

        // Original SSL mode output doesn't work because we have no way
        // of specifying the hostname to check.
        let _ssl_mode = PgSslMode::from_str(&self.pgsslmode)
            .map_err(|_| Error::InvalidSSLMode(self.pgsslmode.to_string()))?;

        Ok(PostgresParams {
            timeout,
            options: PgConnectOptions::new()
                .username(&self.pguser)
                .password(&self.pgpassword)
                .database(&self.pgdatabase)
                .host(&self.pghostaddr)
                .port(port)
                .ssl_root_cert(&self.pgsslrootcert)
                // Override SSL verification mode.
                // Postgres libraries support both PGHOST and PGHOSTADDR in order to manually override
                // the IP address while still verifying the SSL hostname. sqlx does not support this.
                // Instead, set SSL verification mode to CA only, which is probably what we want.
                .ssl_mode(PgSslMode::VerifyCa),
        })
    }
}
