//! Own reception, replay, activity and posting decisions independently of Qt.
use super::{Comments, PresentationStatus, activity::Activity, replay};
use crate::{channels::Channel, playback, services::Network, settings::CommentCacheLimit};
use std::time::Instant;
use viewer_comments::{Comment, cache::Reception, posting};

pub(crate) enum Target<'a> {
    Live(Option<&'a Channel>),
    Recording,
}

/// Only an active/connecting input supplies media observations. A stopped
/// recording still suppresses live reception, including after a failed stop.
pub(crate) enum Playback<'a> {
    Inactive(Target<'a>),
    Connecting(Target<'a>, &'a playback::Session),
    Active(Target<'a>, &'a playback::Session, playback::timeline::Phase),
}
impl Playback<'_> {
    fn channel(&self) -> Option<&Channel> {
        let (Self::Inactive(target) | Self::Connecting(target, _) | Self::Active(target, _, _)) =
            self;
        match target {
            Target::Live(channel) => *channel,
            Target::Recording => None,
        }
    }
    fn seeking(&self) -> bool {
        matches!(
            self,
            Self::Active(_, _, playback::timeline::Phase::Seeking(_))
        )
    }
    fn context(&self, options: Options, reception: Reception) -> Option<replay::Context> {
        use playback::timeline::{Phase, Resume};
        let media = match self {
            Self::Inactive(_) => return None,
            Self::Connecting(_, media) | Self::Active(_, media, _) => media,
        };
        let source = media.source_identity()?;
        let position = media.playback().and_then(|playback| playback.position());
        let view = position
            .map(|position| media.metadata(position))
            .unwrap_or_default();
        let fallback = self.channel().and_then(|channel| channel.broadcast);
        Some(replay::Context {
            source,
            service: view.service.or(fallback),
            position: position.map(|position| position.nseconds()),
            clock: view.clock,
            source_range: media
                .comment_source(fallback, super::channel_for, reception)
                .unwrap_or(viewer_comments::cache::Source::Pending),
            enabled: options.enabled,
            display: options.display,
            paused: matches!(
                self,
                Self::Active(_, _, Phase::Paused | Phase::Seeking(Resume::Paused))
            ),
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Options {
    pub enabled: bool,
    pub display: bool,
    pub cache_limit: CommentCacheLimit,
}

pub(crate) struct Input<'a> {
    pub options: Options,
    pub playback: Playback<'a>,
    pub channels: &'a [Channel],
    pub network: Option<&'a Network>,
    pub now: Instant,
    pub wall_ms: i64,
}

pub(crate) enum Command {
    Poll,
    Submit,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Submission {
    NotRequested,
    Accepted,
    Rejected,
}

pub(crate) enum History {
    Replace(Vec<Comment>),
    Append(Vec<Comment>),
}
pub(crate) struct Update {
    pub submission: Submission,
    pub history: History,
    pub activity: Option<Result<String, serde_json::Error>>,
    pub timeline_changed: bool,
}
pub(crate) enum Status<'a> {
    Reception(PresentationStatus<'a>),
    Replay(&'a replay::Status),
}
#[derive(Default)]
enum StatusSource {
    #[default]
    Disabled,
    Reception,
    Playback,
}

#[derive(Default)]
pub(crate) struct Session {
    reception: Comments,
    replay: replay::Replay,
    activity: Activity,
    draft: String,
    program_title: String,
    status_source: StatusSource,
}

impl Session {
    /// Submission always revalidates selection and invalidates stale drafts in
    /// the same call, before any presentation callback can change the target.
    pub fn update(&mut self, input: Input<'_>, command: Command) -> Update {
        let Input {
            options,
            playback: input,
            channels,
            network,
            now,
            wall_ms,
        } = input;
        self.activity
            .configure(options.enabled && !channels.is_empty());
        if let Some(network) = network {
            self.activity.poll(network);
        }
        let activity = if self.activity.dirty {
            self.activity.dirty = false;
            Some(self.activity.json(channels))
        } else {
            None
        };
        self.replay.set_budget(options.cache_limit.bytes());
        let channel = input.channel();
        let reset = self.reception.configure(options.enabled, channel);
        if reset {
            self.draft.clear();
        }
        self.program_title = self.activity.program_title(channel).to_owned();
        let accepting = self.replay.can_receive();
        let comments = match network.filter(|_| accepting) {
            Some(network) => match self.reception.poll(network) {
                Ok(comments) => comments,
                Err(error) => {
                    tracing::error!(
                        error = &error as &dyn std::error::Error,
                        "Comment reception poll failed"
                    );
                    Vec::new()
                }
            },
            None => Vec::new(),
        };
        let reception = if accepting
            && comments.len() < viewer_comments::controller::MAX_POLL_COMMENTS
            && options.enabled
        {
            self.reception
                .reception_epoch()
                .map(Reception::Receiving)
                .unwrap_or(Reception::Interrupted)
        } else {
            Reception::Interrupted
        };
        let received = comments
            .iter()
            .map(|comment| {
                (
                    comment.clone(),
                    self.reception.posting.is_own_comment(comment, now),
                )
            })
            .collect();
        let previous = self.replay.timeline().clone();
        self.replay.update(
            input.context(options, reception),
            input.seeking() && options.enabled,
            received,
            wall_ms,
        );
        let timeline_changed = !previous.same_snapshot(self.replay.timeline());
        if self.reception.posting.poll(now) {
            self.draft.clear();
        }
        self.status_source = if !options.enabled {
            StatusSource::Disabled
        } else if matches!(input, Playback::Active(..)) {
            StatusSource::Playback
        } else {
            StatusSource::Reception
        };
        let submission = match command {
            Command::Poll => Submission::NotRequested,
            Command::Submit => {
                if network.is_some_and(|network| {
                    network.post_comment(&mut self.reception.posting, &self.draft)
                }) {
                    Submission::Accepted
                } else {
                    Submission::Rejected
                }
            }
        };
        Update {
            submission,
            history: if reset {
                History::Replace(comments)
            } else {
                History::Append(comments)
            },
            activity,
            timeline_changed,
        }
    }

    pub fn status(&self) -> Status<'_> {
        match self.status_source {
            StatusSource::Disabled => Status::Reception(PresentationStatus::Disabled),
            StatusSource::Reception => Status::Reception(self.reception.status(true)),
            StatusSource::Playback => Status::Replay(&self.replay.status),
        }
    }
    pub fn timeline(&self) -> &replay::Timeline {
        self.replay.timeline()
    }
    pub fn disk_bytes(&self) -> u64 {
        self.replay.disk_bytes()
    }
    pub fn program_title(&self) -> &str {
        &self.program_title
    }
    pub fn draft(&self) -> &str {
        &self.draft
    }
    pub fn edit_draft(&mut self, text: String) {
        if !self.posting_busy() && text.len() <= viewer_comments::MAX_COMMENT_BYTES {
            self.draft = text;
        }
    }
    pub fn posting_status(&self) -> &posting::Status {
        self.reception.posting.status()
    }
    pub fn posting_busy(&self) -> bool {
        self.reception.posting.busy()
    }
    pub fn posting_available(&self, network: Option<&Network>, now: Instant) -> bool {
        network.is_some() && self.reception.posting.available(now)
    }
    pub fn posting_channel(&self) -> Option<u16> {
        self.reception.jikkyo_id()
    }
    pub fn open_cache(&mut self) {
        self.replay.open_cache();
    }
    pub fn clear_unused(&mut self) {
        self.replay.clear_unused();
    }
    pub fn set_cache_limit(&mut self, limit: CommentCacheLimit) {
        self.replay.set_budget(limit.bytes());
    }
    pub fn refresh_current(&mut self) {
        self.replay.refresh_current();
    }
    pub fn catalog_changed(&mut self) {
        self.activity.dirty = true;
    }

    /// A server replacement cancels reception/posting and activity. Persistent
    /// replay retains its independent lifetime until the next playback update.
    pub fn disconnect(&mut self) -> Update {
        self.reception.configure(false, None);
        self.activity.configure(false);
        self.activity.dirty = false;
        self.draft.clear();
        self.program_title.clear();
        self.status_source = StatusSource::Disabled;
        Update {
            submission: Submission::NotRequested,
            history: History::Replace(Vec::new()),
            activity: Some(Ok("[]".to_owned())),
            timeline_changed: false,
        }
    }
    pub fn shutdown(&mut self) -> Update {
        let mut update = self.disconnect();
        let previous = self.replay.timeline().clone();
        self.replay.shutdown();
        update.timeline_changed = !previous.same_snapshot(self.replay.timeline());
        update
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels() -> Vec<Channel> {
        // Different services can share one commentary channel. Changing services
        // must still invalidate the old draft and history.
        crate::channels::parse(
            br#"[
            {"id":1,"type":1,"name":"A","networkId":4,"serviceId":101},
            {"id":2,"type":1,"name":"B","networkId":4,"serviceId":101}
        ]"#,
        )
        .unwrap()
    }
    fn input<'a>(channels: &'a [Channel], playback: Playback<'a>) -> Input<'a> {
        Input {
            options: Options {
                enabled: true,
                display: true,
                cache_limit: Default::default(),
            },
            playback,
            channels,
            network: None,
            now: Instant::now(),
            wall_ms: 0,
        }
    }

    #[test]
    fn submission_revalidates_service_before_reading_the_draft() {
        let channels = channels();
        let mut session = Session::default();
        let first = || {
            input(
                &channels,
                Playback::Inactive(Target::Live(Some(&channels[0]))),
            )
        };
        assert!(matches!(
            session.update(first(), Command::Poll).history,
            History::Replace(_)
        ));
        session.edit_draft("draft for A".into());
        assert!(matches!(
            session.update(first(), Command::Poll).history,
            History::Append(_)
        ));
        assert_eq!(session.draft(), "draft for A");
        let second = input(
            &channels,
            Playback::Inactive(Target::Live(Some(&channels[1]))),
        );
        let result = session.update(second, Command::Submit);
        assert_eq!(result.submission, Submission::Rejected);
        assert!(matches!(result.history, History::Replace(_)));
        assert!(session.draft().is_empty());
        assert_eq!(session.posting_channel(), Some(101));
    }

    #[test]
    fn recording_disable_and_disconnect_invalidate_live_posting() {
        let channels = channels();
        let live = || {
            input(
                &channels,
                Playback::Inactive(Target::Live(Some(&channels[0]))),
            )
        };
        let mut session = Session::default();
        session.update(live(), Command::Poll);
        session.edit_draft("live draft".into());
        let recording = input(&channels, Playback::Inactive(Target::Recording));
        session.update(recording, Command::Submit);
        assert!(session.draft().is_empty());
        assert_eq!(session.posting_channel(), None);

        session.update(live(), Command::Poll);
        session.edit_draft("draft before disable".into());
        let mut disabled = live();
        disabled.options.enabled = false;
        session.update(disabled, Command::Poll);
        assert!(session.draft().is_empty());
        assert_eq!(session.posting_channel(), None);
        assert!(matches!(
            session.status(),
            Status::Reception(PresentationStatus::Disabled)
        ));

        session.update(live(), Command::Poll);
        session.edit_draft("draft before disconnect".into());
        session.disconnect();
        assert!(session.draft().is_empty());
        assert_eq!(session.posting_channel(), None);
        assert!(session.program_title().is_empty());
    }

    #[test]
    fn playback_status_and_display_do_not_retarget_live_reception() {
        use playback::timeline::{Phase, Resume};
        let channels = channels();
        // No native pipeline: these orchestration decisions require no Qt or devices.
        let media = playback::Session::new(None, Default::default());
        let mut session = Session::default();
        let connecting = input(
            &channels,
            Playback::Connecting(Target::Live(Some(&channels[0])), &media),
        );
        session.update(connecting, Command::Poll);
        assert!(matches!(
            session.status(),
            Status::Reception(PresentationStatus::Connecting)
        ));
        session.edit_draft("retained while pausing".into());
        let mut seeking = input(
            &channels,
            Playback::Active(
                Target::Live(Some(&channels[0])),
                &media,
                Phase::Seeking(Resume::Paused),
            ),
        );
        seeking.options.display = false;
        let result = session.update(seeking, Command::Poll);
        assert!(matches!(result.history, History::Append(_)));
        assert!(matches!(session.status(), Status::Replay(_)));
        assert_eq!(session.draft(), "retained while pausing");
        assert_eq!(session.posting_channel(), Some(101));
        assert!(!session.posting_available(None, Instant::now()));
    }
}
