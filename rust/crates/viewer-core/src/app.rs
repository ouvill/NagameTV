//! Immutable snapshots and the application's event/effect protocol.
use crate::{
    audio::{AudioOption, AudioProgram},
    channels::{Channel, ChannelCatalog},
    epg::{EpgSnapshot, Program},
    settings::{Settings, normalize_language},
    subtitles::SubtitleCue,
};
use std::{collections::VecDeque, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub summary: String,
    pub details: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Connecting,
    Playing,
    Failed(Failure),
}
impl PlaybackState {
    pub fn active(&self) -> bool {
        matches!(self, Self::Connecting | Self::Playing)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub time: String,
    pub text: String,
    pub source: String,
}

/// Fields are private: callers can only obtain shared views and dispatch events.
/// Large payloads are shared across snapshots; transitions never clone the EPG.
#[derive(Clone)]
pub struct State {
    preferences: Settings,
    selected: Option<u64>,
    playback: PlaybackState,
    status: String,
    catalog: Arc<ChannelCatalog>,
    epg: Arc<EpgSnapshot>,
    now_ms: u64,
    request: Option<u64>,
    next_request: u64,
    server_generation: u64,
    last_refresh: Option<u64>,
    audio_muted: bool,
    audio_tracks: Arc<[AudioOption]>,
    audio_error: String,
    subtitle: Option<Arc<SubtitleCue>>,
    comments: Arc<VecDeque<Comment>>,
    comment_status: String,
    comment_channel: Option<String>,
    comment_generation: u64,
    comment_connected: bool,
    ui_language: String,
    autoplay: bool,
    started: bool,
}

impl State {
    pub fn new(preferences: Settings, ui_language: String, autoplay: bool) -> Self {
        let preferences = preferences.normalized();
        let selected = preferences
            .service_id
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0);
        Self {
            preferences,
            selected,
            ui_language,
            autoplay,
            started: false,
            playback: PlaybackState::Stopped,
            status: "Ready".into(),
            catalog: Arc::new(ChannelCatalog {
                channels: vec![],
                guide: vec![],
                guide_start: 0,
            }),
            epg: Arc::default(),
            now_ms: 0,
            request: None,
            next_request: 0,
            server_generation: 0,
            last_refresh: None,
            audio_muted: false,
            audio_tracks: Arc::default(),
            audio_error: String::new(),
            subtitle: None,
            comments: Arc::default(),
            comment_status: "Select a channel".into(),
            comment_channel: None,
            comment_generation: 0,
            comment_connected: false,
        }
    }
    pub fn preferences(&self) -> &Settings {
        &self.preferences
    }
    pub fn selected(&self) -> Option<u64> {
        self.selected
    }
    pub fn playback(&self) -> &PlaybackState {
        &self.playback
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn catalog(&self) -> &Arc<ChannelCatalog> {
        &self.catalog
    }
    pub fn epg(&self) -> &Arc<EpgSnapshot> {
        &self.epg
    }
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }
    pub fn loading(&self) -> bool {
        self.request.is_some()
    }
    pub fn server_generation(&self) -> u64 {
        self.server_generation
    }
    pub fn comment_generation(&self) -> u64 {
        self.comment_generation
    }
    pub fn audio_muted(&self) -> bool {
        self.audio_muted
    }
    pub fn audio_tracks(&self) -> &[AudioOption] {
        &self.audio_tracks
    }
    pub fn audio_error(&self) -> &str {
        &self.audio_error
    }
    pub fn subtitle(&self) -> Option<&Arc<SubtitleCue>> {
        self.subtitle.as_ref()
    }
    pub fn comments(&self) -> &Arc<VecDeque<Comment>> {
        &self.comments
    }
    pub fn comment_status(&self) -> &str {
        &self.comment_status
    }
    pub fn ui_language(&self) -> &str {
        &self.ui_language
    }
    pub fn autoplay(&self) -> bool {
        self.autoplay
    }
    pub fn selected_channel(&self) -> Option<&Channel> {
        self.catalog
            .channels
            .iter()
            .find(|c| Some(c.id) == self.selected)
    }
    pub fn current_program(&self) -> Option<&Program> {
        self.epg.current_program(self.selected?, self.now_ms)
    }
    pub fn progress(&self) -> f64 {
        self.current_program()
            .filter(|p| p.duration > 0)
            .map_or(0.0, |p| {
                (self.now_ms.saturating_sub(p.start_at) as f64 / p.duration as f64).clamp(0.0, 1.0)
            })
    }
    pub fn audio_program(&self) -> Option<AudioProgram> {
        self.current_program().map(|p| AudioProgram {
            service_id: p.service_id,
            start_at: p.start_at,
            audios: p.audios.clone(),
        })
    }
    pub fn effective_volume(&self) -> f64 {
        if self.audio_muted {
            0.0
        } else {
            self.preferences.volume
        }
    }
}

pub enum Preference {
    Volume(f64),
    Muted(bool),
    Comments(bool),
    CommentFontSize(f64),
    CommentOpacity(f64),
    CommentSpeed(f64),
    Subtitles(bool),
}

/// All timestamps and IO outcomes are explicit inputs. No wall clock in update().
pub enum Event {
    Tick(u64),
    VideoReady,
    ConnectServer(String),
    Refresh {
        force: bool,
    },
    CatalogLoaded {
        request: u64,
        result: Result<(Arc<ChannelCatalog>, Arc<EpgSnapshot>), String>,
    },
    EpgChanged {
        generation: u64,
    },
    SelectChannel(usize),
    ChangeChannel(i32),
    Play,
    Stop,
    PlaybackStarted,
    PlaybackStopped,
    PlaybackObserved,
    PlaybackFailed(Failure),
    Preference(Preference),
    SaveSettings,
    SettingsSaved(Result<(), String>),
    ChangeLanguage(String),
    LanguageApplied {
        preference: String,
        result: Result<String, String>,
    },
    SelectAudio(String),
    AudioObserved {
        tracks: Vec<AudioOption>,
        error: String,
    },
    SubtitlesApplied {
        enabled: bool,
        result: Result<(), Failure>,
    },
    CommentsEnded {
        generation: u64,
    },
    SubtitleObserved(Option<Arc<SubtitleCue>>),
    CommentReceived {
        generation: u64,
        comment: Comment,
        initial: bool,
    },
    CommentStatus {
        generation: u64,
        status: String,
        connected: bool,
    },
    InitializationFailed(String),
}

pub enum Effect {
    LoadCatalog {
        request: u64,
        server: String,
    },
    WatchEpg {
        generation: u64,
        server: String,
    },
    CancelCatalog,
    StartPlayback {
        server: String,
        service: u64,
        program: Option<AudioProgram>,
    },
    StopPlayback,
    CleanupPlayback,
    SetAudioProgram(Option<AudioProgram>),
    SetVolume(f64),
    SetSubtitles(bool),
    SelectAudio(String),
    ConnectComments {
        generation: u64,
        channel: Option<String>,
    },
    PresentComment(String),
    SaveSettings(Settings),
    ApplyLanguage(String),
    Record(&'static str),
    WritePlaybackError(String),
}

pub struct Transition {
    pub accepted: bool,
    pub state: State,
    pub effects: Vec<Effect>,
}

pub fn update(state: &State, event: Event) -> Transition {
    let mut next = state.clone();
    let mut effects = Vec::new();
    let mut accepted = true;
    match event {
        Event::Tick(now) => {
            next.now_ms = now;
            if state
                .current_program()
                .map(|p| (p.service_id, p.start_at, &p.audios))
                != next
                    .current_program()
                    .map(|p| (p.service_id, p.start_at, &p.audios))
            {
                effects.push(Effect::SetAudioProgram(next.audio_program()));
            }
            if next.started
                && next
                    .last_refresh
                    .is_none_or(|last| now.saturating_sub(last) >= 300_000)
            {
                refresh(&mut next, &mut effects, false);
            }
        }
        Event::VideoReady => {
            if !next.started {
                next.started = true;
                effects.push(Effect::SetVolume(next.effective_volume()));
                effects.push(Effect::SetSubtitles(next.preferences.subtitles_enabled));
                effects.push(Effect::WatchEpg {
                    generation: next.server_generation,
                    server: next.preferences.server.clone(),
                });
                refresh(&mut next, &mut effects, true);
                if next.autoplay {
                    play(&mut next, &mut effects);
                }
            }
        }
        Event::ConnectServer(server) => {
            let server = server.trim().trim_end_matches('/').to_owned();
            if !url::Url::parse(&server)
                .is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some())
            {
                accepted = false;
                next.status = "Enter a server URL starting with http:// or https://".into();
            } else {
                if server != next.preferences.server {
                    next.server_generation += 1;
                    next.preferences.server = server.clone();
                    next.preferences.service_id.clear();
                    next.selected = None;
                    next.playback = PlaybackState::Stopped;
                    next.subtitle = None;
                    next.audio_tracks = Arc::default();
                    next.audio_error.clear();
                    next.epg = Arc::default();
                    next.catalog = Arc::new(ChannelCatalog {
                        channels: vec![],
                        guide: vec![],
                        guide_start: 0,
                    });
                    next.request = None;
                    next.last_refresh = None;
                    effects.extend([
                        Effect::CleanupPlayback,
                        Effect::CancelCatalog,
                        Effect::WatchEpg {
                            generation: next.server_generation,
                            server,
                        },
                    ]);
                    restart_comments(&mut next, &mut effects, true);
                }
                effects.push(Effect::SaveSettings(next.preferences.clone()));
                refresh(&mut next, &mut effects, true);
            }
        }
        Event::Refresh { force } => refresh(&mut next, &mut effects, force),
        Event::EpgChanged { generation } => {
            if generation == next.server_generation {
                refresh(&mut next, &mut effects, true);
            }
        }
        Event::CatalogLoaded { request, result } => {
            if next.request == Some(request) {
                next.request = None;
                match result {
                    Ok((catalog, epg)) => {
                        next.catalog = catalog;
                        next.epg = epg;
                        effects.push(Effect::SetAudioProgram(next.audio_program()));
                        if !next.playback.active() {
                            next.status = if next.catalog.channels.is_empty() {
                                "No available channels were found"
                            } else {
                                "Ready"
                            }
                            .into();
                        }
                        restart_comments(&mut next, &mut effects, false);
                        effects.push(Effect::Record("epg_fetch_finished"));
                    }
                    Err(error) => {
                        next.status = error;
                        effects.push(Effect::Record("epg_fetch_failed"));
                    }
                }
            }
        }
        Event::SelectChannel(index) => select(&mut next, &mut effects, index),
        Event::ChangeChannel(offset) => {
            let ids = next
                .catalog
                .channels
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>();
            if let Some(index) = crate::selection::adjacent_index(&ids, next.selected, offset) {
                select(&mut next, &mut effects, index as usize);
            }
        }
        Event::Play => play(&mut next, &mut effects),
        Event::Stop => {
            effects.extend([Effect::Record("stop_requested"), Effect::StopPlayback]);
        }
        Event::PlaybackStarted => {
            next.playback = PlaybackState::Connecting;
            next.status = "Connecting...".into();
        }
        Event::PlaybackObserved => {
            if next.playback.active() {
                next.playback = PlaybackState::Playing;
                next.status = "Playing".into();
            }
        }
        Event::PlaybackStopped => {
            next.playback = PlaybackState::Stopped;
            next.status = "Stopped".into();
            next.subtitle = None;
            next.audio_tracks = Arc::default();
            next.audio_error.clear();
        }
        Event::PlaybackFailed(failure) => fail(&mut next, &mut effects, failure),
        Event::Preference(preference) => {
            let previous_volume = next.effective_volume();
            match preference {
                Preference::Volume(v) => {
                    next.preferences.volume = finite(v, 0.0, 100.0, next.preferences.volume)
                }
                Preference::Muted(v) => next.audio_muted = v,
                Preference::Comments(v) => next.preferences.danmaku_enabled = v,
                Preference::CommentFontSize(v) => {
                    next.preferences.comment_font_size =
                        finite(v, 12.0, 48.0, next.preferences.comment_font_size)
                }
                Preference::CommentOpacity(v) => {
                    next.preferences.comment_opacity =
                        finite(v, 0.1, 1.0, next.preferences.comment_opacity)
                }
                Preference::CommentSpeed(v) => {
                    next.preferences.comment_speed =
                        finite(v, 0.5, 2.0, next.preferences.comment_speed)
                }
                Preference::Subtitles(v) => {
                    if v != next.preferences.subtitles_enabled {
                        effects.push(Effect::SetSubtitles(v));
                    }
                }
            }
            if previous_volume != next.effective_volume() {
                effects.push(Effect::SetVolume(next.effective_volume()));
            }
        }
        Event::SaveSettings => effects.push(Effect::SaveSettings(next.preferences.clone())),
        Event::SettingsSaved(result) => {
            if let Err(error) = result {
                next.status = format!("Could not save settings: {error}");
            }
        }
        Event::ChangeLanguage(language) => {
            effects.push(Effect::ApplyLanguage(normalize_language(&language).into()))
        }
        Event::LanguageApplied { preference, result } => match result {
            Ok(language) => {
                next.preferences.language = preference;
                next.ui_language = language;
                effects.push(Effect::SaveSettings(next.preferences.clone()));
            }
            Err(error) => {
                accepted = false;
                next.status = error;
            }
        },
        Event::SelectAudio(key) => effects.push(Effect::SelectAudio(key)),
        Event::AudioObserved { tracks, error } => {
            if next.audio_tracks.as_ref() != tracks {
                next.audio_tracks = Arc::from(tracks);
            }
            next.audio_error = error;
        }
        Event::SubtitlesApplied { enabled, result } => match result {
            Ok(()) => {
                next.preferences.subtitles_enabled = enabled;
                next.subtitle = None;
            }
            Err(error) => fail(&mut next, &mut effects, error),
        },
        Event::CommentsEnded { generation } => {
            if generation == next.comment_generation {
                next.comment_connected = false;
            }
        }
        Event::SubtitleObserved(cue) => {
            next.subtitle = if next.playback.active() && next.preferences.subtitles_enabled {
                cue
            } else {
                None
            };
        }
        Event::CommentReceived {
            generation,
            comment,
            initial,
        } => {
            if generation == next.comment_generation {
                if !initial {
                    effects.push(Effect::PresentComment(comment.text.clone()));
                }
                let history = Arc::make_mut(&mut next.comments);
                if history.len() == 200 {
                    history.pop_front();
                }
                history.push_back(comment);
            }
        }
        Event::CommentStatus {
            generation,
            status,
            connected,
        } => {
            if generation == next.comment_generation {
                next.comment_status = status;
                next.comment_connected = connected;
            }
        }
        Event::InitializationFailed(error) => next.status = error,
    }
    Transition {
        state: next,
        effects,
        accepted,
    }
}

fn finite(value: f64, min: f64, max: f64, previous: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        previous
    }
}

fn refresh(state: &mut State, effects: &mut Vec<Effect>, force: bool) {
    if state.request.is_some()
        || (!force
            && state
                .last_refresh
                .is_some_and(|last| state.now_ms.saturating_sub(last) < 60_000))
    {
        return;
    }
    state.next_request += 1;
    state.request = Some(state.next_request);
    state.last_refresh = Some(state.now_ms);
    state.status = "Loading channels...".into();
    effects.push(Effect::LoadCatalog {
        request: state.next_request,
        server: state.preferences.server.clone(),
    });
    effects.push(Effect::Record("epg_fetch_started"));
}

fn select(state: &mut State, effects: &mut Vec<Effect>, index: usize) {
    let Some(channel) = state.catalog.channels.get(index) else {
        return;
    };
    if crate::selection::SelectionAction::for_request(
        state.selected,
        channel.id,
        state.playback.active(),
    ) == crate::selection::SelectionAction::Keep
    {
        return;
    }
    state.selected = Some(channel.id);
    state.preferences.service_id = channel.id.to_string();
    effects.push(Effect::Record("channel_selected"));
    restart_comments(state, effects, false);
    effects.push(Effect::SaveSettings(state.preferences.clone()));
    play(state, effects);
}

fn play(state: &mut State, effects: &mut Vec<Effect>) {
    state.subtitle = None;
    effects.push(Effect::Record("play_requested"));
    if let Some(service) = state.selected {
        state.playback = PlaybackState::Connecting;
        state.status = "Connecting...".into();
        effects.push(Effect::StartPlayback {
            server: state.preferences.server.clone(),
            service,
            program: state.audio_program(),
        });
    } else {
        fail(
            state,
            effects,
            Failure {
                summary: "Enter a valid Mirakurun service ID".into(),
                details: "Enter a valid Mirakurun service ID".into(),
            },
        );
    }
}

fn fail(state: &mut State, effects: &mut Vec<Effect>, failure: Failure) {
    state.status = failure.details.clone();
    effects.push(Effect::CleanupPlayback);
    effects.push(Effect::WritePlaybackError(failure.details.clone()));
    state.playback = PlaybackState::Failed(failure);
    state.subtitle = None;
    state.audio_tracks = Arc::default();
    state.audio_error.clear();
}

fn restart_comments(state: &mut State, effects: &mut Vec<Effect>, force: bool) {
    let channel = state.selected_channel().and_then(|c| c.jikkyo_id.clone());
    if !force && channel.is_some() && state.comment_channel == channel && state.comment_connected {
        return;
    }
    state.comment_channel = channel.clone();
    state.comment_generation += 1;
    state.comment_connected = channel.is_some();
    state.comments = Arc::default();
    state.comment_status = if channel.is_some() {
        "Connecting to comments…"
    } else {
        "Comments are unavailable for this channel"
    }
    .into();
    effects.push(Effect::ConnectComments {
        generation: state.comment_generation,
        channel,
    });
}

#[cfg(test)]
mod tests;
