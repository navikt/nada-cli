use clap::Subcommand;
use sqlx::postgres::{PgConnectOptions, PgSslMode};
use sqlx::{ConnectOptions, Connection, PgConnection};
use std::process::Stdio;
use std::str::FromStr;
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader, Lines};
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// Configure Nais Postgres for ingestion.
    Enable {
        /// Name of the database to configure.
        #[arg()]
        database: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unable to capture standard output")]
    Stdout,
    #[error("Nais command-line interface timeout")]
    Timeout,
    #[error("Invalid port: {0}")]
    InvalidPort(String),
    #[error("Invalid SSL mode: {0}")]
    InvalidSSLMode(String),
    #[error("OS error: {0}")]
    OS(#[from] std::io::Error),
    #[error("Postgres error: {0}")]
    Postgres(#[from] sqlx::Error),
}

/// A proxied connection to a Nais Postgres instance.
///
/// The proxy is initialized by running `nais postgres proxy`.
///
/// The Nais command-line interface will keep the connection open indefinitely.
/// To drop the proxied connection, callers *MUST* call `drop()` after finishing.
struct PostgresProxy {
    nais_cli_process: tokio::process::Child,
    connection: PgConnection,
}

impl PostgresProxy {
    /// Use the Nais command-line interface to set up a connection proxy to a Nais Postgres instance.
    ///
    /// Note that
    pub async fn new(database: impl AsRef<str>) -> Result<Self, Error> {
        let mut command = Command::new("nais");
        command.arg("postgres");
        command.arg("proxy");
        command.arg("--access-level").arg("admin");
        command.arg("--output").arg("env");
        command.arg("--print-password");
        command
            .arg("--reason")
            .arg("Configuring database for warehouse ingestion");
        command.arg(database.as_ref());

        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::null());

        let mut nais_cli_process = command.spawn()?;

        // Parse Postgres connection parameters from environment variables
        // printed by Nais CLI.
        let stdout = nais_cli_process.stdout.take().ok_or(Error::Stdout)?;
        let lines = BufReader::new(stdout).lines();
        let postgres_params = Self::read_connect_options_from_stdout(lines).await.unwrap();

        // Connect to the Postgres database.
        let connection = postgres_params.connect().await?;

        Ok(Self {
            connection,
            nais_cli_process,
        })
    }

    /// Parse a single line of Nais CLI environment variable output and append it to the Postgres connect options.
    ///
    ///     PGHOST=value
    ///     PGPORT=value
    ///     ...
    ///
    /// Returns the accumulated Postgres connect options if the variable was accepted.
    /// Otherwise, an error is returned.
    ///
    /// Unknown variables are ignored.
    fn parse_stdout_env_into_connect_options(
        mut opts: PgConnectOptions,
        line: impl AsRef<str>,
    ) -> Result<PgConnectOptions, Error> {
        let mut parts = line.as_ref().splitn(2, "=");
        let Some(key) = parts.next() else {
            return Ok(opts);
        };
        let Some(value) = parts.next() else {
            println!("Parse {key}=VALUE: no value");
            return Ok(opts);
        };
        let value = value.trim_matches('\'');
        println!("Captured {key}");
        match key {
            "PGHOST" => opts = opts.host(value), // fixme: use for hostname ssl validation
            "PGHOSTADDR" => opts = opts.host(value),
            "PGPORT" => {
                let port = value
                    .parse()
                    .map_err(|_| Error::InvalidPort(value.to_string()))?;
                opts = opts.port(port);
            }
            "PGUSER" => opts = opts.username(value),
            "PGSSLMODE" => {
                let ssl_mode = PgSslMode::from_str(value)
                    .map_err(|_| Error::InvalidSSLMode(value.to_string()))?;
                opts = opts.ssl_mode(ssl_mode);
            }
            "PGSSLROOTCERT" => opts = opts.ssl_root_cert(value),
            "PGPASSWORD" => opts = opts.password(value),
            "PGCONNECT_TIMEOUT" => {}
            _ => {
                println!("Ignoring environment variable {key} = {value}");
            }
        };
        Ok(opts)
    }

    /// Parse all PG* environment variables from Nais CLI.
    ///
    /// This function times out after a few seconds, in that case an error is returned.
    /// Otherwise, the Postgres connection options are returned.
    ///
    /// The first line from Nais CLI might take a while to appear,
    /// so it has a longer timeout than subsequent lines.
    async fn read_connect_options_from_stdout<B: AsyncBufRead + Unpin>(
        mut lines: Lines<B>,
    ) -> Result<PgConnectOptions, Error> {
        let mut opts = PgConnectOptions::new();

        match timeout(Duration::from_secs(5), lines.next_line()).await {
            Ok(Ok(Some(line))) => opts = Self::parse_stdout_env_into_connect_options(opts, line)?,
            _ => return Err(Error::Timeout),
        };

        loop {
            match timeout(Duration::from_millis(500), lines.next_line()).await {
                Ok(Ok(Some(line))) => {
                    opts = Self::parse_stdout_env_into_connect_options(opts, line)?
                }
                Ok(Ok(None)) => return Ok(opts),
                Ok(Err(e)) => return Err(Error::OS(e)),
                Err(_elapsed) => return Ok(opts),
            }
        }
    }

    async fn close(mut self) -> Result<(), Error> {
        self.connection.close().await?;
        self.nais_cli_process.start_kill()?;
        self.nais_cli_process.wait().await?;
        Ok(())
    }
}

pub async fn connect_and_enable_ingest(database: impl AsRef<str>) -> Result<(), Error> {
    let connection = PostgresProxy::new(database).await?;
    println!("Connected to Nais Postgres");
    connection.close().await?;
    Ok(())
}
