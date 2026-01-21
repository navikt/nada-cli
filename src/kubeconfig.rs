use clap::Subcommand;

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// Update $KUBECONFIG with Nada clusters.
    Update,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("OS error: {0}")]
    OS(#[from] std::io::Error),
    #[error("gcloud subprocess aborted")]
    Killed,
    #[error("gcloud returned non-zero exit code: {0}")]
    ExitCode(i32),
}

struct ClusterReference {
    name: String,
    google_project_name: String,
    location: String,
}

macro_rules! cluster {
    ($name: literal, $goog: literal, $loc: literal) => {
        ClusterReference {
            name: $name.to_string(),
            google_project_name: $goog.to_string(),
            location: $loc.to_string(),
        }
    };
}

pub async fn update_config_file() -> Result<(), Error> {
    for cluster in builtin_clusters() {
        println!("Downloading credentials for cluster '{}' in Google project '{}'...", cluster.name, cluster.google_project_name);

        let status = std::process::Command::new("gcloud")
            .arg("container")
            .arg("clusters")
            .arg("get-credentials")
            .arg(cluster.name)
            .arg("--project")
            .arg(cluster.google_project_name)
            .arg("--location")
            .arg(cluster.location)
            .status()?
            .code()
            .ok_or(Error::Killed)?;

        if status != 0 {
            return Err(Error::ExitCode(status));
        }
    }

    println!("All clusters configured successfully!");

    Ok(())
}

fn builtin_clusters() -> Vec<ClusterReference> {
    vec![
        cluster!("knada-gke", "knada-gcp", "europe-north1"),
        cluster!("knada-gke-dev", "knada-dev", "europe-north1"),
        cluster!("knada-gpu", "knada-dev", "europe-west1"),
    ]
}
