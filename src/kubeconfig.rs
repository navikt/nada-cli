use clap::Subcommand;
use google_cloud_container_v1::model::ListClustersRequest;
use google_cloud_gax::error::rpc::Code;
use google_cloud_gax::paginator::ItemPaginator;
use google_cloud_resourcemanager_v3::model::{ListFoldersRequest, ListProjectsRequest, Project};
use std::collections::HashSet;

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// Update $KUBECONFIG with Nada clusters.
    Update {
        /// Override Google Cloud folders used to look for Kubernetes clusters.
        #[arg(short, long, default_values_t = default_folders())]
        folders: Vec<String>,
        /// Override Google Cloud locations where Kubernetes clusters should be running.
        #[arg(short, long, default_values_t = default_locations())]
        locations: Vec<String>,
    },
}

fn default_folders() -> Vec<String> {
    vec!["739335424622".into(), "105918521196".into()]
}

fn default_locations() -> Vec<String> {
    vec!["europe-north1".into(), "europe-west1".into()]
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("initialize client: {0}")]
    Builder(#[from] google_cloud_gax::client_builder::Error),
    #[error("PAM: {0}")]
    Pam(#[from] google_cloud_privilegedaccessmanager_v1::Error),
    #[error("OS error: {0}")]
    OS(#[from] std::io::Error),
    #[error("Subprocess aborted")]
    Killed,
    #[error("Subprocess returned non-zero exit code: {0}")]
    ExitCode(i32),
}

#[derive(Debug, Hash, Eq, PartialEq)]
struct ClusterReference {
    name: String,
    google_project_name: String,
    location: String,
}

async fn fetch_clusters_in_projects(
    projects: &Vec<Project>,
    locations: &Vec<String>,
) -> Result<HashSet<ClusterReference>, Error> {
    let client = google_cloud_container_v1::client::ClusterManager::builder()
        .build()
        .await?;
    let mut result = HashSet::new();
    for project in projects {
        for location in locations {
            let parent_id = format!("{}/locations/{}", project.name, location);
            let request = ListClustersRequest::new().set_parent(parent_id);
            let resp = match client.list_clusters().with_request(request).send().await {
                Ok(resp) => resp,
                Err(err)
                    if err
                        .status()
                        .is_some_and(|err| err.code == Code::PermissionDenied) =>
                {
                    // Silently ignore "permission denied".
                    // this error code is being used to report that the GKE API is not enabled for that project.
                    // We really don't care because we are traversing a whole bunch of projects.
                    //println!("{:?}: {}", err.status(), err);
                    continue;
                }
                Err(err) => return Err(err.into()),
            };
            for cluster in resp.clusters {
                result.insert(ClusterReference {
                    name: cluster.name,
                    google_project_name: project.project_id.clone(),
                    location: cluster.location,
                });
            }
        }
    }
    Ok(result)
}

async fn fetch_subfolder_ids_recursive(folder_id: impl ToString) -> Result<HashSet<String>, Error> {
    let client = google_cloud_resourcemanager_v3::client::Folders::builder()
        .build()
        .await?;
    let request = ListFoldersRequest::new().set_parent(folder_id.to_string());
    let mut item_iterator = client.list_folders().with_request(request).by_item();
    let mut result = HashSet::new();
    result.insert(folder_id.to_string());
    while let Some(item) = item_iterator.next().await {
        let folder_id = item?.name.to_string();
        result.insert(folder_id.clone());
        let child_folders = Box::pin(fetch_subfolder_ids_recursive(folder_id)).await?;
        result.extend(child_folders);
    }
    Ok(result)
}

async fn fetch_project_list(
    folders: impl IntoIterator<Item = String>,
) -> Result<Vec<Project>, Error> {
    let client = google_cloud_resourcemanager_v3::client::Projects::builder()
        .build()
        .await?;
    let mut result = vec![];
    for folder in folders {
        let request = ListProjectsRequest::new().set_parent(folder);
        let mut item_iterator = client.list_projects().with_request(request).by_item();
        while let Some(item) = item_iterator.next().await {
            result.push(item?);
        }
    }
    Ok(result)
}

pub async fn update_config_file(folders: Vec<String>, locations: Vec<String>) -> Result<(), Error> {
    println!("Enumerating subfolders in {} folders...", folders.len());
    let folder_ids: Vec<String> = folders
        .into_iter()
        .map(|folder_id| format!("folders/{folder_id}"))
        .collect();
    let mut result = HashSet::new();
    for folder_id in folder_ids {
        let folders = fetch_subfolder_ids_recursive(folder_id).await?;
        result.extend(folders);
    }

    println!("Enumerating projects in {} folders...", result.len());
    let projects = fetch_project_list(result).await?;

    println!("Enumerating clusters in {} projects...", projects.len());
    let clusters = fetch_clusters_in_projects(&projects, &locations).await?;

    println!("Found a total of {} clusters to configure.", clusters.len());
    println!("-----");

    for cluster in clusters {
        println!(
            "Downloading credentials for cluster '{}' in Google project '{}'...",
            cluster.name, cluster.google_project_name
        );

        println!("{:?}", cluster);

        gcloud_download_cluster_credentials(&cluster)?;
        kubectx_rename_cluster(&cluster)?;
    }

    println!("All clusters configured successfully!");

    Ok(())
}

fn gcloud_download_cluster_credentials(cluster: &ClusterReference) -> Result<(), Error> {
    match std::process::Command::new("gcloud")
        .arg("container")
        .arg("clusters")
        .arg("get-credentials")
        .arg(&cluster.name)
        .arg("--project")
        .arg(&cluster.google_project_name)
        .arg("--location")
        .arg(&cluster.location)
        .status()?
        .code()
        .ok_or(Error::Killed)?
    {
        0 => Ok(()),
        exit_code => Err(Error::ExitCode(exit_code)),
    }
}

fn kubectx_rename_cluster(cluster: &ClusterReference) -> Result<(), Error> {
    let autogen_name = gcloud_get_autogen_name(cluster);
    let friendly_name = &cluster.name;
    let rename_invocation = format!("{friendly_name}={autogen_name}");
    match std::process::Command::new("kubectx")
        .arg(rename_invocation)
        .status()?
        .code()
        .ok_or(Error::Killed)?
    {
        0 => Ok(()),
        exit_code => Err(Error::ExitCode(exit_code)),
    }
}

fn gcloud_get_autogen_name(cluster: &ClusterReference) -> String {
    let project = &cluster.google_project_name;
    let region = &cluster.location;
    let cluster_name = &cluster.name;
    format!("gke_{project}_{region}_{cluster_name}")
}

#[cfg(test)]
#[test]
fn test_autogen() {
    let cluster = ClusterReference {
        name: "knada-gke".to_string(),
        google_project_name: "knada-gcp".to_string(),
        location: "europe-north1".to_string(),
    };
    let autogen_name = gcloud_get_autogen_name(&cluster);
    assert_eq!("gke_knada-gcp_europe-north1_knada-gke", autogen_name);
}
