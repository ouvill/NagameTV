use crate::epg::{EpgSnapshot, Program, Service};
use serde::Deserialize;
use std::sync::Arc;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub(super) enum FetchServicesError {
    #[error("Could not load channels: {0}")]
    ServicesRequest(#[source] reqwest::Error),
    #[error("Mirakurun rejected the channel request: {0}")]
    ServicesStatus(#[source] reqwest::Error),
    #[error("Invalid Mirakurun service list: {0}")]
    ServicesResponse(#[source] reqwest::Error),
    #[error("Could not load programs: {0}")]
    ProgramsRequest(#[source] reqwest::Error),
    #[error("Mirakurun rejected the program request: {0}")]
    ProgramsStatus(#[source] reqwest::Error),
    #[error("Invalid Mirakurun program list: {0}")]
    ProgramsResponse(#[source] reqwest::Error),
    #[error("Could not read system time: {0}")]
    SystemTime(#[source] std::time::SystemTimeError),
    #[error("Channel request worker stopped without returning a result")]
    WorkerStopped,
}

#[derive(Deserialize)]
struct NxChannel {
    id: String,
    threads: Vec<NxThread>,
}

#[derive(Deserialize)]
struct NxThread {
    status: String,
    jikkyo_force: Option<u64>,
}

pub(super) async fn fetch_services(
    client: &reqwest::Client,
    server: &str,
) -> Result<FetchedCatalog, FetchServicesError> {
    let api = server.trim().trim_end_matches('/');
    let services = client
        .get(format!("{api}/api/services"))
        .send()
        .await
        .map_err(FetchServicesError::ServicesRequest)?
        .error_for_status()
        .map_err(FetchServicesError::ServicesStatus)?
        .json::<Vec<Service>>()
        .await
        .map_err(FetchServicesError::ServicesResponse)?;
    let programs = client
        .get(format!("{api}/api/programs"))
        .send()
        .await
        .map_err(FetchServicesError::ProgramsRequest)?
        .error_for_status()
        .map_err(FetchServicesError::ProgramsStatus)?
        .json::<Vec<Program>>()
        .await
        .map_err(FetchServicesError::ProgramsResponse)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(FetchServicesError::SystemTime)?
        .as_millis() as u64;
    let snapshot = Arc::new(EpgSnapshot::new(services, programs, now));
    let mut catalog = crate::channels::build_catalog(&snapshot, now);
    if let Ok(forces) = fetch_jikkyo_forces(client).await {
        for channel in &mut catalog.channels {
            channel.jikkyo_force = channel
                .jikkyo_id
                .as_ref()
                .and_then(|id| forces.get(id).copied());
        }
    }
    Ok(FetchedCatalog { catalog, snapshot })
}

async fn fetch_jikkyo_forces(
    client: &reqwest::Client,
) -> Result<HashMap<String, u64>, reqwest::Error> {
    let channels = client
        .get("https://nx-jikkyo.tsukumijima.net/api/v1/channels")
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<NxChannel>>()
        .await?;
    Ok(channels
        .into_iter()
        .filter_map(|channel| {
            channel
                .threads
                .into_iter()
                .find(|thread| thread.status == "ACTIVE")
                .and_then(|thread| thread.jikkyo_force)
                .map(|force| (channel.id, force))
        })
        .collect())
}

pub(super) struct FetchedCatalog {
    pub catalog: crate::channels::ChannelCatalog,
    pub snapshot: Arc<EpgSnapshot>,
}
