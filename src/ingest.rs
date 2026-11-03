use clap::Subcommand;
use std::io::{BufRead, BufReader, Lines};
use std::process::Stdio;
use std::time::Duration;

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
    #[error("unable to set up standard input")]
    Stdin,
    #[error("unable to capture standard output")]
    Stdout,
    #[error("OS error: {0}")]
    OS(#[from] std::io::Error),
    #[error("Subprocess aborted")]
    Killed,
    #[error("Subprocess returned non-zero exit code: {0}")]
    ExitCode(i32),
}

#[derive(Debug, Default)]
struct PostgresParams {
    host: String,
    host_address: String,
    port: u16,
    username: String,
    password: String,
    ssl_mode: String,
    ssl_root_certificate_path: String,
    connect_timeout: Duration,
}

impl PostgresParams {
    fn new_from_nais_env<B: BufRead>(lines: Lines<B>) -> Self {
        let mut me = Self::default();
        for line in lines {
            let Ok(line) = line else {
                break;
            };
            let mut parts = line.splitn(2, "=");
            let Some(key) = parts.next() else {
                continue;
            };
            let Some(value) = parts.next() else {
                println!("Parse {key}=VALUE: no value");
                continue;
            };
            let value = value.trim_matches('\'');
            println!("{key} = {value}");
            match key {
                "PGHOST" => me.host = value.to_string(),
                "PGHOSTADDR" => me.host_address = value.to_string(),
                "PGPORT" => me.port = value.parse().unwrap(),
                "PGUSER" => me.username = value.to_string(),
                "PGSSLMODE" => me.ssl_mode = value.to_string(),
                "PGSSLROOTCERT" => me.ssl_root_certificate_path = value.to_string(),
                "PGCONNECT_TIMEOUT" => {
                    me.connect_timeout = value.parse().map(Duration::from_secs).unwrap()
                }
                "PGPASSWORD" => me.password = value.to_string(),
                _ => {
                    println!("Ignoring unknown environment variable {key} = {value}");
                }
            };
            if me.is_valid() {
                break;
            }
        }
        me
    }

    fn is_valid(&self) -> bool {
        !(self.host.is_empty()
            || self.host_address.is_empty()
            || self.username.is_empty()
            || self.password.is_empty()
            || self.ssl_mode.is_empty()
            || self.ssl_root_certificate_path.is_empty()
            || self.port == 0
            || self.connect_timeout.is_zero())
    }
}

pub async fn connect_and_enable_ingest(database: impl AsRef<str>) -> Result<(), Error> {
    let mut command = std::process::Command::new("nais");
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

    let mut process = command.spawn()?;
    let stdout = process.stdout.take().ok_or(Error::Stdout)?;
    let stdin = process.stdin.take().ok_or(Error::Stdin)?;

    let stdout = BufReader::new(stdout);
    let lines = stdout.lines();

    let params = PostgresParams::new_from_nais_env(lines);
    println!("{params:?}");

    // closing pipe should end the program
    drop(stdin);

    // fixme
    //process.wait()?;
    process.kill()?;

    Ok(())
}
