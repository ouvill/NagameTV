//! Executes core effects and feeds their results back as events. No QObject/QML types.
mod catalog_request;
mod fetch;
mod telemetry;

use crate::{
    network::{NetworkRuntime, NetworkTask},
    playback::{Playback, PlaybackError, PlaybackEvent},
};
use catalog_request::CatalogRequest;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use viewer_core::app::{self, Comment, Effect, Event, Failure, State};

pub enum Presentation {
    Comment(String),
    ApplyLanguage(String),
}

pub struct Runtime {
    state: State,
    playback: Option<Playback>,
    network: Option<NetworkRuntime>,
    catalog_request: CatalogRequest,
    catalog_id: u64,
    epg_task: Option<NetworkTask>,
    epg_tx: mpsc::SyncSender<u64>,
    epg_rx: mpsc::Receiver<u64>,
    comment_task: Option<NetworkTask>,
    comment_generation: Arc<AtomicU64>,
    comment_tx: mpsc::SyncSender<(u64, crate::comments::CommentEvent)>,
    comment_rx: mpsc::Receiver<(u64, crate::comments::CommentEvent)>,
    presentation: Vec<Presentation>,
    log_directory: Option<PathBuf>,
    recorder: Option<crate::diagnostics::Recorder>,
    ui: telemetry::UiState,
}

impl Runtime {
    /// Qt provides only platform-specific presentation information.
    pub fn new(ui_language: String, log_directory: Option<PathBuf>) -> Self {
        let log_directory = log_directory
            .filter(|path| path.is_absolute())
            .and_then(|path| {
                std::fs::create_dir_all(&path)
                    .map_err(|error| tracing::warn!(%error, "Could not prepare log directory"))
                    .ok()
                    .map(|_| path)
            });
        let mut settings = crate::settings::load().unwrap_or_else(|error| {
            tracing::warn!(%error, "Could not load settings");
            Default::default()
        });
        if let Ok(server) = std::env::var("MIRAKURUN_SERVER") {
            settings.server = server;
        }
        if let Ok(service) = std::env::var("MIRAKURUN_SERVICE_ID") {
            settings.service_id = service;
        }
        let autoplay = std::env::var("MIRAKURUN_AUTOPLAY").is_ok_and(|v| v != "0");
        let mut state = State::new(settings, ui_language, autoplay);
        let playback = crate::playback::take_preloaded().and_then(|playback| {
            playback.set_subtitles_enabled(state.preferences().subtitles_enabled)?;
            Ok(playback)
        });
        let network = NetworkRuntime::new();
        if let Some(error) = playback
            .as_ref()
            .err()
            .map(ToString::to_string)
            .or_else(|| network.as_ref().err().map(ToString::to_string))
        {
            state = app::update(&state, Event::InitializationFailed(error)).state;
        }
        if let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) {
            state = app::update(
                &state,
                Event::Tick(u64::try_from(now.as_millis()).unwrap_or(u64::MAX)),
            )
            .state;
        }
        let (epg_tx, epg_rx) = mpsc::sync_channel(1);
        let (comment_tx, comment_rx) = mpsc::sync_channel(crate::comments::QUEUE_CAPACITY);
        let recorder = if std::env::var("MIRAKURUN_DIAGNOSTICS").is_ok_and(|v| v == "0") {
            None
        } else {
            log_directory.as_ref().and_then(|p| {
                crate::diagnostics::Recorder::start(p.join("usage"))
                    .map_err(|error| tracing::warn!(%error, "Could not start passive diagnostics"))
                    .ok()
            })
        };
        Self {
            state,
            playback: playback.ok(),
            network: network.ok(),
            catalog_request: Default::default(),
            catalog_id: 0,
            epg_task: None,
            epg_tx,
            epg_rx,
            comment_task: None,
            comment_generation: Arc::new(AtomicU64::new(0)),
            comment_tx,
            comment_rx,
            presentation: vec![],
            log_directory,
            recorder,
            ui: Default::default(),
        }
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn take_presentation(&mut self) -> Vec<Presentation> {
        std::mem::take(&mut self.presentation)
    }
    pub fn dispatch(&mut self, event: Event) {
        self.request(event);
    }
    pub fn request(&mut self, event: Event) -> bool {
        let mut accepted = true;
        let mut events = VecDeque::from([event]);
        while let Some(event) = events.pop_front() {
            let transition = app::update(&self.state, event);
            accepted &= transition.accepted;
            self.state = transition.state;
            for effect in transition.effects {
                self.execute(effect, &mut events);
            }
        }
        accepted
    }
    fn execute(&mut self, effect: Effect, events: &mut VecDeque<Event>) {
        match effect {
            Effect::LoadCatalog { request, server } => {
                if let Some(network) = &self.network {
                    self.catalog_id = request;
                    self.catalog_request =
                        CatalogRequest::start(&network.handle(), network.client(), server);
                } else {
                    events.push_back(Event::CatalogLoaded {
                        request,
                        result: Err("Network runtime is unavailable".into()),
                    });
                }
            }
            Effect::CancelCatalog => self.catalog_request.cancel(),
            Effect::WatchEpg { generation, server } => {
                self.epg_task = None;
                while self.epg_rx.try_recv().is_ok() {}
                if let Some(network) = &self.network {
                    self.epg_task = Some(NetworkTask(network.handle().spawn(
                        crate::epg_events::receive(
                            network.stream_client(),
                            server,
                            generation,
                            self.epg_tx.clone(),
                        ),
                    )));
                }
            }
            Effect::StartPlayback {
                server,
                service,
                program,
            } => {
                let result = self
                    .playback
                    .as_ref()
                    .ok_or_else(unavailable)
                    .and_then(|p| p.play_service(&server, service, program).map_err(failure));
                events.push_back(
                    result.map_or_else(Event::PlaybackFailed, |_| Event::PlaybackStarted),
                );
            }
            Effect::StopPlayback => {
                let result = self
                    .playback
                    .as_ref()
                    .ok_or_else(unavailable)
                    .and_then(|p| p.stop().map_err(failure));
                events.push_back(
                    result.map_or_else(Event::PlaybackFailed, |_| Event::PlaybackStopped),
                );
            }
            Effect::CleanupPlayback => {
                if let Some(p) = &self.playback
                    && let Err(error) = p.stop()
                {
                    tracing::warn!(%error, "Could not clean up playback");
                }
            }
            Effect::SetAudioProgram(program) => {
                if let Some(p) = &self.playback {
                    p.set_audio_program(program);
                }
            }
            Effect::SetVolume(volume) => {
                if let Some(p) = &self.playback {
                    p.set_volume(volume);
                }
            }
            Effect::SetSubtitles(enabled) => {
                let result = self
                    .playback
                    .as_ref()
                    .ok_or_else(unavailable)
                    .and_then(|p| p.set_subtitles_enabled(enabled).map_err(failure));
                events.push_back(Event::SubtitlesApplied { enabled, result });
            }
            Effect::SelectAudio(key) => {
                if let Some(p) = &self.playback {
                    p.select_audio_option(&key);
                    let (tracks, error) = p.audio_state();
                    events.push_back(Event::AudioObserved { tracks, error });
                }
            }
            Effect::ConnectComments {
                generation,
                channel,
            } => {
                self.comment_generation.store(generation, Ordering::Release);
                self.comment_task = None;
                while self.comment_rx.try_recv().is_ok() {}
                if let Some(channel) = channel {
                    if let Some(network) = &self.network {
                        self.comment_task = Some(NetworkTask(network.handle().spawn(
                            crate::comments::receive(
                                network.client(),
                                channel,
                                generation,
                                self.comment_generation.clone(),
                                self.comment_tx.clone(),
                            ),
                        )));
                    } else {
                        events.push_back(Event::CommentStatus {
                            generation,
                            status: "Network runtime is unavailable".into(),
                            connected: false,
                        });
                    }
                }
            }
            Effect::PresentComment(text) => self.presentation.push(Presentation::Comment(text)),
            Effect::SaveSettings(settings) => events.push_back(Event::SettingsSaved(
                crate::settings::save(&settings).map_err(|e| e.to_string()),
            )),
            Effect::ApplyLanguage(preference) => self
                .presentation
                .push(Presentation::ApplyLanguage(preference)),
            Effect::Record(event) => self.record(event),
            Effect::WritePlaybackError(message) => {
                tracing::error!(%message, "Playback failed");
                if let Some(path) = &self.log_directory
                    && let Err(error) =
                        std::fs::write(path.join("playback-error.log"), format!("{message}\n"))
                {
                    tracing::warn!(%error, "Could not save playback diagnostic");
                }
            }
        }
    }
    /// Platform event pumps call this; scheduling policy lives in the core.
    pub fn tick(&mut self) {
        if let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) {
            self.dispatch(Event::Tick(
                u64::try_from(now.as_millis()).unwrap_or(u64::MAX),
            ));
        }
        if self.state.playback().active() {
            match self
                .playback
                .as_ref()
                .ok_or_else(unavailable)
                .and_then(|p| p.drain_events().map_err(failure))
            {
                Ok(PlaybackEvent::None) => {}
                Ok(PlaybackEvent::Playing) => self.dispatch(Event::PlaybackObserved),
                Ok(PlaybackEvent::Ended) => {
                    self.dispatch(Event::PlaybackFailed(failure(PlaybackError::StreamEnded)))
                }
                Err(error) => self.dispatch(Event::PlaybackFailed(error)),
            }
        }
        if let Some(p) = &self.playback {
            let (tracks, error) = p.audio_state();
            if tracks != self.state.audio_tracks() || error != self.state.audio_error() {
                self.dispatch(Event::AudioObserved { tracks, error });
            }
        }
        if let Some((_, result)) = self.catalog_request.poll() {
            self.dispatch(Event::CatalogLoaded {
                request: self.catalog_id,
                result: result
                    .map(|r| (Arc::new(r.catalog), r.snapshot))
                    .map_err(|e| e.to_string()),
            });
        }
        if !self.state.loading()
            && let Ok(generation) = self.epg_rx.try_recv()
        {
            self.dispatch(Event::EpgChanged { generation });
        }
        let comments = self.comment_rx.try_iter().take(64).collect::<Vec<_>>();
        for (generation, event) in comments {
            self.dispatch(match event {
                crate::comments::CommentEvent::Status(status) => Event::CommentStatus {
                    generation,
                    status,
                    connected: self
                        .comment_task
                        .as_ref()
                        .is_some_and(|t| !t.0.is_finished()),
                },
                crate::comments::CommentEvent::Comment {
                    time,
                    text,
                    source,
                    initial,
                } => Event::CommentReceived {
                    generation,
                    comment: Comment { time, text, source },
                    initial,
                },
            });
        }
        if self
            .comment_task
            .as_ref()
            .is_some_and(|task| task.0.is_finished())
        {
            self.comment_task = None;
            self.dispatch(Event::CommentsEnded {
                generation: self.comment_generation.load(Ordering::Acquire),
            });
        }
    }

    pub fn poll_subtitles(&mut self) -> bool {
        let Some(playback) = &self.playback else {
            return false;
        };
        let event = match playback.poll_subtitles() {
            crate::subtitles::SubtitleUpdate::Unchanged => return false,
            crate::subtitles::SubtitleUpdate::Clear => Event::SubtitleObserved(None),
            crate::subtitles::SubtitleUpdate::Show(cue) => {
                Event::SubtitleObserved(Some(Arc::new(cue)))
            }
        };
        self.dispatch(event);
        true
    }
    pub fn log_directory(&self) -> Option<&std::path::Path> {
        self.log_directory.as_deref()
    }
    /// The adapter keeps the live GUI-owned video item alive for the sink.
    pub fn attach_video(&mut self, address: *mut std::ffi::c_void) -> bool {
        match self
            .playback
            .as_mut()
            .ok_or_else(unavailable)
            .and_then(|p| p.attach_video_item(address).map_err(failure))
        {
            Ok(()) => true,
            Err(error) => {
                self.dispatch(Event::PlaybackFailed(error));
                false
            }
        }
    }
    pub fn video_stats(&self) -> String {
        self.playback
            .as_ref()
            .and_then(|p| serde_json::to_string(&p.video_stats()).ok())
            .unwrap_or_else(|| "{}".into())
    }
}
fn unavailable() -> Failure {
    Failure {
        summary: "Could not initialize the player".into(),
        details: "Could not initialize the player".into(),
    }
}
fn failure(error: PlaybackError) -> Failure {
    Failure {
        summary: error.user_message().into(),
        details: error.to_string(),
    }
}

#[cfg(test)]
mod tests;
