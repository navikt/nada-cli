/// Nada's global GCP folder.
const GLOBAL_FOLDER: &'static str = "folders/739335424622/locations/global";

use chrono::{DateTime, Timelike, Utc};
use clap::Subcommand;
use google_cloud_gax::paginator::ItemPaginator;
use google_cloud_privilegedaccessmanager_v1::client::PrivilegedAccessManager;
use google_cloud_privilegedaccessmanager_v1::model::justification::Justification::UnstructuredJustification;
use google_cloud_privilegedaccessmanager_v1::model::{
    CreateGrantRequest, Entitlement, Grant, Justification,
};
use google_cloud_wkt::Timestamp;
use std::cmp::Ordering;
use std::io::Write;
use std::str::FromStr;
use std::time::Duration;
use tabwriter::TabWriter;

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// List possible privilege entitlements.
    Entitlements,
    /// List my elevated privileges.
    List,
    /// Grant elevated privileges to me.
    Grant {
        /// Entitlement identifier, as seen at Google.
        #[arg(short, long)]
        entitlement: Option<String>,
        /// How long you need elevated privileges.
        #[arg(short, long)]
        duration: Option<humantime::Duration>,
        /// Reason for requesting elevaletd privileges.
        #[arg(short, long)]
        reason: Option<String>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("initialize client: {0}")]
    Builder(#[from] google_cloud_gax::client_builder::Error),
    #[error("request failed: {0}")]
    Request(#[from] google_cloud_privilegedaccessmanager_v1::Error),
    #[error("no entitlements have been set up")]
    NoEntitlements,
    #[error("user cancelled the operation")]
    UserCancelled,
    #[error("value out of range")]
    OutOfRange,
}

pub async fn entitlements() -> Result<(), Error> {
    for entitlement in fetch_entitlement_list().await? {
        println!("{}", entitlement.name);
    }
    Ok(())
}

pub async fn grants() -> Result<(), Error> {
    let mut output = TabWriter::new(std::io::stdout());
    let _ = write!(&mut output, "ID\tSTATE\tTIME\tDURATION_SECS\tREASON\n");
    for grant in fetch_grant_list().await? {
        let grant_id = grant
            .name
            .split("/")
            .last()
            .map(|grant_id| grant_id.to_string())
            .unwrap_or_default();
        let justification = grant.justification.and_then(|just| just.justification);
        let justification_text = match justification {
            Some(UnstructuredJustification(x)) => x,
            _ => "".to_string(),
        };
        let time_text = grant
            .create_time
            .and_then(timestamp_to_date)
            .and_then(|x| x.with_nanosecond(0))
            .map(|x| x.naive_local())
            .map(|x| x.to_string())
            .unwrap_or("???".to_string());
        let _ = write!(
            &mut output,
            "{}\t{}\t{}\t{}\t{}\n",
            grant_id,
            grant.state,
            time_text,
            grant.requested_duration.unwrap_or_default().seconds(),
            justification_text,
        );
    }
    let _ = output.flush();
    Ok(())
}

pub async fn grant_using_dialog(
    entitlement_id: Option<impl ToString>,
    duration_secs: Option<u64>,
    reason: Option<impl ToString>,
) -> Result<(), Error> {
    println!("Prepare to request privilege escalation");
    println!();

    let entitlement_id = match entitlement_id {
        Some(s) => s.to_string(),
        None => prompt_entitlement_stdin().await?,
    };

    let duration = match duration_secs {
        Some(seconds) => google_cloud_wkt::Duration::new(seconds as i64, 0).unwrap_or_default(),
        None => prompt_duration_stdin(),
    };

    let reason = match reason {
        Some(reason) => reason.to_string(),
        None => prompt_reason_stdin(),
    };

    println!("Ready to elevate privileges");
    println!();
    println!("Entitlement...: {}", entitlement_id);
    println!("Duration......: {} seconds", duration.seconds());
    println!("Reason........: {}", reason);
    println!();

    loop {
        print!("Is the above information correct? (y/n): ");
        std::io::stdout().flush().unwrap();
        match read_bool_stdin() {
            Some(true) => {
                println!();
                println!("Requesting privilege escalation...");
                println!();
                let grant = create_grant(&entitlement_id, duration, &reason).await?;
                println!();
                println!("Success! Your privileges have been escalated!");
                println!("Grant ID: {}", grant.name);
                println!();
                println!("With great power comes great responsibility.");
                println!("Think before you type!");
                println!();
                return Ok(());
            }
            Some(false) => return Err(Error::UserCancelled),
            None => {
                println!("Please answer either 'y' or 'n'.")
            }
        }
    }
}

fn read_duration_stdin() -> Option<Duration> {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).ok()?;
    humantime::parse_duration(line.trim()).ok()
}

fn read_string_stdin() -> Option<String> {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).ok()?;
    Some(line.trim().to_string())
}

fn read_int_stdin() -> Option<usize> {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).ok()?;
    usize::from_str(line.trim()).ok()
}

fn read_bool_stdin() -> Option<bool> {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).ok()?;
    match line.trim() {
        "Y" | "y" => Some(true),
        "N" | "n" => Some(false),
        _ => None,
    }
}

async fn prompt_entitlement_stdin() -> Result<String, Error> {
    println!("List of possible entitlements");
    println!("=============================");
    let entitlements = fetch_entitlement_list().await?;
    for (index, entitlement) in entitlements.iter().enumerate() {
        println!("{:2}. {}", index + 1, entitlement.name);
    }
    println!();
    loop {
        print!("Select an entitlement (1..{}): ", entitlements.len());
        std::io::stdout().flush().unwrap();
        let Some(input) = read_int_stdin() else {
            println!("Invalid choice, please select a valid number");
            continue;
        };
        if input <= 0 || input > entitlements.len() {
            println!(
                "Out of range, please select a valid number between 1-{}",
                entitlements.len()
            );
            continue;
        }
        return entitlements
            .get(input.saturating_sub(1))
            .map(|x| x.name.to_string())
            .ok_or(Error::OutOfRange);
    }
}

fn prompt_duration_stdin() -> google_cloud_wkt::Duration {
    loop {
        print!("Enter privilege escalation duration (minimum 30 minutes): ");
        std::io::stdout().flush().unwrap();
        let Some(duration) = read_duration_stdin() else {
            println!("Invalid duration; please enter a number followed by a unit (s, m, h).");
            continue;
        };
        return google_cloud_wkt::Duration::new(duration.as_secs() as i64, 0).unwrap_or_default();
    }
}

fn prompt_reason_stdin() -> String {
    loop {
        print!("Enter reason for privilege escalation: ");
        std::io::stdout().flush().unwrap();
        match read_string_stdin() {
            Some(s) if !s.is_empty() => {
                return s;
            }
            _ => {
                println!("Invalid reason; must be a non-empty string.");
            }
        }
    }
}

async fn fetch_entitlement_list() -> Result<Vec<Entitlement>, Error> {
    let client = PrivilegedAccessManager::builder().build().await?;
    let mut result = vec![];
    let mut item_iterator = client
        .list_entitlements()
        .set_parent(GLOBAL_FOLDER)
        .by_item();
    while let Some(item) = item_iterator.next().await {
        result.push(item?);
    }
    match result.is_empty() {
        true => Err(Error::NoEntitlements),
        false => Ok(result),
    }
}

async fn fetch_grant_list() -> Result<Vec<Grant>, Error> {
    let mut result = vec![];
    let client = PrivilegedAccessManager::builder().build().await?;
    for entitlement in fetch_entitlement_list().await? {
        let mut item_iterator = client.list_grants().set_parent(&entitlement.name).by_item();
        while let Some(item) = item_iterator.next().await {
            result.push(item?);
        }
    }
    result.sort_by(|a, b| {
        b.create_time
            .partial_cmp(&a.create_time)
            .unwrap_or(Ordering::Greater)
    });
    Ok(result)
}

async fn create_grant(
    entitlement_id: impl ToString,
    duration: google_cloud_wkt::Duration,
    reason: impl ToString,
) -> Result<Grant, Error> {
    let client = PrivilegedAccessManager::builder().build().await?;
    let justification = Justification::new().set_unstructured_justification(reason.to_string());
    let grant = Grant::new()
        .set_requested_duration(duration)
        .set_justification(justification);
    let req = CreateGrantRequest::new().set_grant(grant);
    Ok(client
        .create_grant()
        .with_request(req)
        .set_parent(entitlement_id.to_string())
        .send()
        .await?)
}

fn timestamp_to_date(ts: Timestamp) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(ts.seconds(), ts.nanos() as u32)
}
