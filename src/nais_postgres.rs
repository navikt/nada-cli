use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::{Pool, Postgres};
use std::process::Stdio;
use std::str::FromStr;
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader, Lines};
use tokio::process::Command;
use tokio::time::timeout;
use tracing::log::debug;

/// A proxied connection to a Nais Postgres instance.
///
/// The proxy is initialized by running `nais postgres proxy`.
///
/// The Nais command-line interface will keep the connection open indefinitely.
/// To drop the proxied connection, callers should call `close()` after finishing.
/// Otherwise, both the Postgres and proxy connections will linger until the program finishes.
pub struct PostgresProxy {
    nais_cli_process: tokio::process::Child,
    pool: Pool<Postgres>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unable to capture standard output")]
    Stdout,
    #[error("timeout while waiting for configuration")]
    Timeout,
    #[error("invalid port: {0}")]
    InvalidPort(String),
    #[error("invalid SSL mode: {0}")]
    InvalidSSLMode(String),
    #[error("OS: {0}")]
    OS(#[from] std::io::Error),
    #[error("Postgres: {0}")]
    Postgres(#[from] sqlx::Error),
}

impl PostgresProxy {
    /// How long to wait for Nais CLI to print the first line of configuration.
    const NAIS_CLI_TIMEOUT_FIRST_OUTPUT: Duration = Duration::from_secs(10);

    /// How long to wait for Nais CLI to print subsequent lines of configuration.
    /// After this timeout is reached, the program considers Nais CLI ready to proxy connections.
    const NAIS_CLI_TIMEOUT_SUBSEQUENT_OUTPUTS: Duration = Duration::from_millis(500);

    /// Reason for Postgres privilege escalation.
    /// This reason might be used in a official court.
    const NAIS_CLI_JITA_REASON: &'static str = "Configuring database for warehouse ingestion";

    /// Connection timeout to Nais Postgres.
    const NAIS_POSTGRES_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

    /// Use the Nais command-line interface to set up a connection proxy to a Nais Postgres instance.
    pub async fn new(database: impl AsRef<str>) -> Result<Self, Error> {
        let mut command = Command::new("nais");

        command
            .arg("postgres")
            .arg("proxy")
            .arg("--print-password")
            .arg("--access-level")
            .arg("admin")
            .arg("--output")
            .arg("env")
            .arg("--reason")
            .arg(Self::NAIS_CLI_JITA_REASON)
            .arg(database.as_ref());

        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::null());

        let mut nais_cli_process = command.spawn()?;

        let stdout = nais_cli_process.stdout.take().ok_or(Error::Stdout)?;
        let lines = BufReader::new(stdout).lines();
        let mut postgres_params = Self::read_connect_options_from_stdout(lines).await?;

        // Override SSL verification mode.
        // Postgres libraries support both PGHOST and PGHOSTADDR in order to manually override
        // the IP address while still verifying the SSL hostname. sqlx does not support this.
        // Instead, set SSL verification mode to CA only, which is probably what we want.
        postgres_params = postgres_params.ssl_mode(PgSslMode::VerifyCa);

        // fixme: Override Postgres database; this should probably be set by Nais CLI.
        postgres_params = postgres_params.database("app");

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Self::NAIS_POSTGRES_CONNECT_TIMEOUT)
            .connect_with(postgres_params)
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
        self.nais_cli_process.start_kill()?;
        self.nais_cli_process.wait().await?;
        Ok(())
    }

    /// Parse a single line of Nais CLI environment variable output,
    /// and merge it with the given Postgres connect options.
    ///
    /// If a known variable cannot be parsed, an error is returned.
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
            debug!("Parse {key}=VALUE: no value");
            return Ok(opts);
        };

        // fixme: trim single quotes, should be fixed by nais cli
        let value = value.trim_matches('\'');

        debug!("Capturing environment variable {key} from Nais CLI");

        match key {
            "PGHOSTADDR" => opts = opts.host(value),
            "PGPORT" => {
                opts = opts.port(
                    value
                        .parse()
                        .map_err(|_| Error::InvalidPort(value.to_string()))?,
                );
            }
            "PGDATABASE" => opts = opts.database(value),
            "PGUSER" => opts = opts.username(value),
            "PGSSLMODE" => {
                opts = opts.ssl_mode(
                    PgSslMode::from_str(value)
                        .map_err(|_| Error::InvalidSSLMode(value.to_string()))?,
                );
            }
            "PGSSLROOTCERT" => opts = opts.ssl_root_cert(value),
            "PGPASSWORD" => opts = opts.password(value),
            "PGHOST" => {}            // intentionally ignored, see below.
            "PGCONNECT_TIMEOUT" => {} // timeouts cannot be configured in PgConnectOptions

            _ => {
                debug!("Ignoring environment variable {key} = {value}");
            }
        };

        Ok(opts)
    }

    /// Parse all PG* environment variables from Nais CLI.
    ///
    ///     PGHOST=value
    ///     PGPORT=value
    ///     ...
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

        match timeout(Self::NAIS_CLI_TIMEOUT_FIRST_OUTPUT, lines.next_line()).await {
            Ok(Ok(Some(line))) => opts = Self::parse_stdout_env_into_connect_options(opts, line)?,
            _ => return Err(Error::Timeout),
        };

        loop {
            match timeout(Self::NAIS_CLI_TIMEOUT_SUBSEQUENT_OUTPUTS, lines.next_line()).await {
                Ok(Ok(Some(line))) => {
                    opts = Self::parse_stdout_env_into_connect_options(opts, line)?
                }
                Ok(Ok(None)) => return Ok(opts),
                Ok(Err(e)) => return Err(Error::OS(e)),
                Err(_elapsed) => return Ok(opts),
            }
        }
    }
}
