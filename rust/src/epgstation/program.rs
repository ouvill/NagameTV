//! Optional catalogue information follows the active recording, independently of playback I/O.
use super::{Connection, Endpoint, Error, Library, Recording, client};
use crate::{
    services::{FetchError, Job, Network, Progress, Stopping},
    transport::programs::{
        catalog::View,
        presentation::{Cache, Data},
    },
    viewing_history::recordings::Source,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Information {
    name: String,
    description: String,
    extended: String,
    station: String,
    start: i64,
    end: i64,
    genres: Vec<(u8, u8)>,
}
impl From<&Recording> for Information {
    fn from(record: &Recording) -> Self {
        Self {
            name: record.name.clone(),
            description: record.description.clone(),
            extended: record.extended.clone(),
            station: record.channel.clone(),
            start: record.start_ms,
            end: record.end_ms,
            genres: record.genres.clone(),
        }
    }
}
impl Information {
    fn data(&self) -> Data {
        let description = if self.extended.is_empty() {
            self.description.clone()
        } else {
            format!("{}\n\n{}", self.description, self.extended)
                .trim()
                .to_owned()
        };
        Data::new(serde_json::json!({
            "source":"epgstation", "name":self.name, "description":description,
            "extended":self.extended, "station":self.station, "provider":"",
            "startAt":self.start, "duration":self.end.checked_sub(self.start),
            "genres":self.genres, "progressKnown":false,
            "playbackStartMs":null, "playbackEndMs":null
        }))
    }
}

pub(crate) struct Request {
    key: Source,
    seed: Option<Information>,
    connection: Option<Connection>,
}
impl Library {
    pub(crate) fn program_request(
        &self,
        file: &crate::playback::recording::Recording,
    ) -> Option<Request> {
        let key = file.history_identity().source;
        let Source::Epgstation {
            server,
            recorded_id,
            ..
        } = &key
        else {
            return None;
        };
        let seed = file.program().cloned().or_else(|| {
            (self.server() == Some(server.as_str()))
                .then(|| {
                    self.rows()
                        .iter()
                        .find(|row| row.id == *recorded_id)
                        .map(Information::from)
                })
                .flatten()
        });
        let connection = self
            .session
            .as_ref()
            .filter(|(endpoint, _)| endpoint.as_str() == server)
            .map(|(_, connection)| connection.clone());
        Some(Request {
            key,
            seed,
            connection,
        })
    }
}
impl Request {
    async fn fetch(self) -> Result<Information, FetchError<Error>> {
        let Source::Epgstation {
            server,
            recorded_id,
            ..
        } = self.key
        else {
            unreachable!("only EPGStation requests")
        };
        let connection = match self.connection {
            Some(connection) => connection,
            None => Connection::anonymous(Endpoint::parse(&server).map_err(FetchError::Parse)?)?,
        };
        client::recorded(connection, recorded_id).await
    }
}

#[derive(Default)]
enum Work {
    #[default]
    Idle,
    Fetching(Job<Information, Error>),
    Retiring(Stopping),
}
#[derive(Default)]
pub(crate) struct Fallback {
    key: Option<Source>,
    pending: Option<Request>,
    work: Work,
    data: Option<Data>,
}
impl Fallback {
    pub(crate) fn synchronize(
        &mut self,
        file: Option<&crate::playback::recording::Recording>,
        library: &Library,
    ) {
        let key = file
            .and_then(|file| file.stored_history())
            .map(|history| &history.source)
            .filter(|source| matches!(source, Source::Epgstation { .. }));
        if self.key.as_ref() != key {
            self.configure(file.and_then(|file| library.program_request(file)));
        }
    }
    pub(crate) fn configure(&mut self, request: Option<Request>) {
        if self.key.as_ref() == request.as_ref().map(|request| &request.key) {
            return;
        }
        self.work = match std::mem::take(&mut self.work) {
            Work::Fetching(job) => Work::Retiring(job.cancel()),
            work @ (Work::Idle | Work::Retiring(_)) => work,
        };
        self.key = request.as_ref().map(|request| request.key.clone());
        self.data = request
            .as_ref()
            .and_then(|request| request.seed.as_ref())
            .map(Information::data);
        self.pending = request.filter(|request| request.seed.is_none());
    }
    pub(crate) fn poll(&mut self, network: Option<&Network>) {
        self.work = match std::mem::take(&mut self.work) {
            Work::Idle => Work::Idle,
            Work::Fetching(job) => match job.poll() {
                Progress::Pending(job) => Work::Fetching(job),
                Progress::Complete(result) => {
                    match result {
                        Ok(program) => self.data = Some(program.data()),
                        Err(error) => log_failure(error),
                    }
                    Work::Idle
                }
            },
            Work::Retiring(job) => match job.poll() {
                Progress::Pending(job) => Work::Retiring(job),
                Progress::Complete(()) => Work::Idle,
            },
        };
        if matches!(self.work, Work::Idle)
            && let Some(request) = self.pending.take()
        {
            if let Some(network) = network {
                self.work = Work::Fetching(network.job(request.fetch()));
            } else {
                tracing::warn!(
                    operation = "fetch recording program information",
                    "Network is unavailable"
                );
            }
        }
    }
    pub(crate) fn project(
        &self,
        enabled: bool,
        view: &View,
        position_ns: u64,
        cache: &mut Cache,
    ) -> (Option<Data>, f64, &'static str) {
        if !enabled {
            return (None, 0.0, "disabled");
        }
        let (data, progress) = view.project(position_ns, cache);
        if data.is_some() {
            return (data, progress, view.status.as_str());
        }
        if let Some(data) = &self.data {
            return (Some(data.clone()), 0.0, "available");
        }
        (None, 0.0, view.status.as_str())
    }
    #[cfg(any(test, feature = "native_tests"))]
    pub(crate) fn busy(&self) -> bool {
        self.pending.is_some() || !matches!(self.work, Work::Idle)
    }
}
fn log_failure(error: FetchError<Error>) {
    // serde diagnostics can quote response values, including private tokens.
    fn log(error: &(dyn std::error::Error + 'static)) {
        tracing::warn!(
            operation = "fetch recording program information",
            error,
            "Recording program information unavailable"
        );
    }
    match error {
        FetchError::Parse(Error::Json(error)) => log(&super::reservations::JsonError(error)),
        error => log(&error),
    }
}

#[cfg(test)]
mod tests;
