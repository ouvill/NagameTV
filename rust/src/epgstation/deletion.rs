//! A confirmation owns one catalogue identity and can submit it only once.
use super::{Availability, Catalogue, Error, Library, Network, Progress, client};
use crate::services::{FetchError, Job};
use crate::viewing_history::recordings::Source;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub(crate) enum DeleteError {
    #[error("The recording is no longer in the current list")]
    Missing,
    #[error("Wait for the recording list to finish loading")]
    CatalogueBusy,
    #[error("Stop playback or opening the recording before deleting it")]
    Playback,
    #[error("EPGStation is still recording this program")]
    Recording,
    #[error("EPGStation protects this recording from deletion")]
    Protected,
    #[error("The recording changed on EPGStation. Refresh the list and confirm again")]
    Changed,
    #[error("Network is unavailable")]
    Network,
    #[error(transparent)]
    Request(#[from] FetchError<Error>),
}

#[derive(Default)]
pub(super) enum Deletion {
    #[default]
    Disabled,
    Enabled(State),
}

#[derive(Default)]
pub(super) enum State {
    #[default]
    Idle,
    Confirming(Target),
    Deleting {
        id: u64,
        job: Job<(), DeleteError>,
    },
    Deleted,
    Failed(DeleteError),
}

/// Owned by the confirmation. QML cannot replace the ID at submission time.
pub(super) struct Target {
    pub(super) id: u64,
    pub(super) start_ms: i64,
    pub(super) title: String,
    server: String,
}

pub(crate) enum DeletePlayback {
    Idle,
    Inspecting,
    Open(Source),
}

impl DeletePlayback {
    fn blocks(&self, target: &Target) -> bool {
        match self {
            Self::Idle | Self::Open(Source::Local { .. }) => false,
            // A manually opened HTTP URL need not carry an EPGStation identity.
            Self::Inspecting | Self::Open(Source::Http { .. }) => true,
            Self::Open(Source::Epgstation {
                server,
                recorded_id,
                ..
            }) => server == &target.server && *recorded_id == target.id,
        }
    }
}

impl Deletion {
    pub(super) fn locked(&self) -> bool {
        matches!(
            self,
            Self::Enabled(State::Confirming(_) | State::Deleting { .. })
        )
    }
}

impl Library {
    pub(crate) fn with_deletion(enabled: bool) -> Self {
        Self {
            deletion: if enabled {
                Deletion::Enabled(State::Idle)
            } else {
                Deletion::Disabled
            },
            ..Self::default()
        }
    }

    pub(crate) fn deletion_enabled(&self) -> bool {
        matches!(self.deletion, Deletion::Enabled(_))
    }

    pub(crate) fn deletion_state(&self) -> &'static str {
        match self.deletion {
            Deletion::Disabled | Deletion::Enabled(State::Idle) => "idle",
            Deletion::Enabled(State::Confirming(_)) => "confirming",
            Deletion::Enabled(State::Deleting { .. }) => "deleting",
            Deletion::Enabled(State::Deleted) => "deleted",
            Deletion::Enabled(State::Failed(_)) => "failed",
        }
    }

    pub(crate) fn deletion_error(&self) -> Option<&DeleteError> {
        match &self.deletion {
            Deletion::Enabled(State::Failed(error)) => Some(error),
            _ => None,
        }
    }

    pub(crate) fn deleting(&self) -> bool {
        matches!(self.deletion, Deletion::Enabled(State::Deleting { .. }))
    }

    pub(crate) fn prepare_deletion(&mut self, id: u64, playback: &DeletePlayback) -> bool {
        if !self.deletion_enabled() || self.deletion.locked() {
            return false;
        }
        let result = self.deletion_target(id, playback);
        self.deletion = Deletion::Enabled(match result {
            Ok(target) => State::Confirming(target),
            Err(error) => failed(id, error),
        });
        true
    }

    fn deletion_target(&self, id: u64, playback: &DeletePlayback) -> Result<Target, DeleteError> {
        if self.busy() {
            return Err(DeleteError::CatalogueBusy);
        }
        let Catalogue::Loaded { query, page, .. } = &self.catalogue else {
            return Err(DeleteError::Missing);
        };
        let row = page
            .rows
            .iter()
            .find(|row| row.id == id)
            .ok_or(DeleteError::Missing)?;
        if matches!(row.availability, Availability::Recording) {
            return Err(DeleteError::Recording);
        }
        let target = Target {
            id,
            start_ms: row.start_ms,
            title: row.name.clone(),
            server: query.endpoint.as_str().to_owned(),
        };
        if playback.blocks(&target) {
            return Err(DeleteError::Playback);
        }
        Ok(target)
    }

    pub(crate) fn confirm_deletion(
        &mut self,
        network: Option<&Network>,
        playback: &DeletePlayback,
    ) -> bool {
        let Deletion::Enabled(state @ State::Confirming(_)) = &mut self.deletion else {
            return false;
        };
        let State::Confirming(target) = std::mem::take(state) else {
            unreachable!();
        };
        let id = target.id;
        *state = match (network, &self.catalogue) {
            _ if playback.blocks(&target) => failed(id, DeleteError::Playback),
            (
                Some(network),
                Catalogue::Loaded {
                    connection, query, ..
                },
            ) if query.endpoint.as_str() == target.server => {
                let connection = connection.clone();
                let job = network.job(async move {
                    client::delete_recorded(connection, target)
                        .await
                        .map_err(FetchError::Parse)
                });
                State::Deleting { id, job }
            }
            (None, _) => failed(id, DeleteError::Network),
            _ => failed(id, DeleteError::Missing),
        };
        true
    }

    pub(crate) fn dismiss_deletion(&mut self) -> bool {
        let Deletion::Enabled(state @ (State::Confirming(_) | State::Failed(_) | State::Deleted)) =
            &mut self.deletion
        else {
            return false;
        };
        *state = State::Idle;
        true
    }

    pub(super) fn poll_deletion(&mut self, network: &Network) {
        let Deletion::Enabled(state @ State::Deleting { .. }) = &mut self.deletion else {
            return;
        };
        let State::Deleting { id, job } = std::mem::take(state) else {
            unreachable!();
        };
        match job.poll() {
            Progress::Pending(job) => *state = State::Deleting { id, job },
            Progress::Complete(Err(FetchError::Parse(error))) => *state = failed(id, error),
            Progress::Complete(Err(FetchError::Network(error))) => {
                *state = failed(id, DeleteError::Request(error.into()));
            }
            Progress::Complete(Ok(())) => {
                *state = State::Deleted;
                self.remove_deleted(network, id);
            }
        }
    }

    fn remove_deleted(&mut self, network: &Network, id: u64) {
        let Catalogue::Loaded {
            page,
            selection,
            query,
            ..
        } = &mut self.catalogue
        else {
            unreachable!("catalogue is retained during a deletion");
        };
        page.rows = page
            .rows
            .iter()
            .filter(|row| row.id != id)
            .cloned()
            .collect::<Arc<[_]>>();
        page.total = page.total.saturating_sub(1);
        *selection = std::mem::take(selection).retained(page);
        self.revision = self.revision.wrapping_add(1);
        if page.rows.is_empty() && query.offset > 0 {
            // Refresh corrects the offset when the last page disappears.
            self.refresh(network);
        }
    }
}

fn failed(id: u64, error: DeleteError) -> State {
    tracing::warn!(
        recorded_id = id,
        error = &error as &dyn std::error::Error,
        "EPGStation recording deletion failed"
    );
    State::Failed(error)
}
