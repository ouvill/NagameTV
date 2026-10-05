//! Prepare a complete stream projection, commit it, then notify Qt observers.
//! Snapshot declarations keep each observed value beside its notification(s).
use super::{PlaybackStatus, PlayerRust, ffi, stream_state::State};
use crate::playback;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

macro_rules! snapshot {
    ($($field:ident: $ty:ty = |$player:ident| $read:expr => [$($signal:ident),+];)+) => {
        struct Snapshot { $($field: $ty,)+ }
        impl Snapshot {
            fn capture(player: &ffi::Player) -> Self {
                Self { $($field: { let $player = player; $read },)+ }
            }
            fn notify_changes(&self, after: &Self, mut player: Pin<&mut ffi::Player>) {
                $(if self.$field != after.$field {
                    $(player.as_mut().$signal();)+
                })+
            }
        }
    };
}

snapshot! {
    data_broadcast_available: bool = |p| p.data_broadcast_available() => [data_broadcast_available_changed];
    data_broadcast_requested: bool = |p| p.data_broadcast_requested() => [data_broadcast_requested_changed];
    data_broadcast_activate: bool = |p| p.data_broadcast_activate() => [data_broadcast_activate_changed];
    data_broadcast_endpoint: QString = |p| p.data_broadcast_endpoint().clone() => [data_broadcast_endpoint_changed];
    seek_preview_revision: u64 = |p| p.rust().seek_preview.revision() => [seek_preview_image_changed];
    applied_rate: playback::speed::Rate = |p| p.rust().speed.applied => [playback_rate_changed];
    requested_rate: playback::speed::Rate = |p| p.rust().speed.requested => [requested_playback_rate_changed];
    availability: playback::speed::Availability = |p| p.rust().speed.availability => [speed_available_changed, speed_reason_changed];
    at_live_edge: bool = |p| p.at_live_edge() => [at_live_edge_changed];
    comment_post_available: bool = |p| *p.comment_post_available() => [comment_post_available_changed];
    action: ffi::PlaybackAction = |p| p.playback_action() => [playback_action_changed];
    timeshift: bool = |p| p.timeshift() => [timeshift_changed];
    pausable: bool = |p| p.pausable() => [pausable_changed];
    window_start_ms: f64 = |p| p.window_start_ms() => [window_start_ms_changed];
    live_delay_ms: f64 = |p| p.live_delay_ms() => [live_delay_ms_changed];
    window_end_ms: f64 = |p| p.window_end_ms() => [window_end_ms_changed];
    duration_estimated: bool = |p| p.duration_estimated() => [duration_estimated_changed];
    viewing_channel: i32 = |p| p.viewing_channel() => [viewing_channel_changed];
    live_timeline: QString = |p| p.live_timeline().clone() => [live_timeline_changed];
    bytes_per_second: f64 = |p| *p.timeshift_bytes_per_second() => [timeshift_bytes_per_second_changed];
    current_program: QString = |p| p.current_program_data().clone() => [current_program_data_changed];
    program_status: QString = |p| p.program_status().clone() => [program_status_changed];
    program_progress: f64 = |p| *p.program_progress() => [program_progress_changed];
    subtitle: QString = |p| p.subtitle_data().clone() => [subtitle_data_changed];
    connecting: bool = |p| p.connecting() => [connecting_changed];
    playing: bool = |p| p.playing() => [playing_changed];
    recording: bool = |p| p.recording() => [recording_changed];
    recording_name: QString = |p| p.recording_name() => [recording_name_changed];
    media_active: bool = |p| p.media_active() => [media_active_changed];
    paused: bool = |p| p.paused() => [paused_changed];
    seeking: bool = |p| p.seeking() => [seeking_changed];
    ended: bool = |p| p.ended() => [ended_changed];
    seekable: bool = |p| p.seekable() => [seekable_changed];
    position_ms: f64 = |p| p.position_ms() => [position_ms_changed];
    seek_target_ms: f64 = |p| p.seek_target_ms() => [seek_target_ms_changed];
    duration_ms: f64 = |p| p.duration_ms() => [duration_ms_changed];
}

struct Values {
    stream_state: State,
    timeline: playback::timeline::Snapshot,
    speed: playback::speed::Snapshot,
    timeshift_bytes_per_second: f64,
    live_timeline: QString,
    current_program_data: QString,
    program_progress: f64,
    program_status: QString,
    subtitle_data: QString,
    subtitle_cells: usize,
}

impl Values {
    fn prepare(
        this: &mut PlayerRust,
        before: &Snapshot,
        change: impl FnOnce(State) -> State,
    ) -> Self {
        let mut next = Self {
            stream_state: change(std::mem::take(&mut this.stream_state)),
            timeline: this.timeline,
            speed: this.speed,
            timeshift_bytes_per_second: 0.0,
            live_timeline: this.live_timeline.clone(),
            current_program_data: this.current_program_data.clone(),
            program_progress: this.program_progress,
            program_status: this.program_status.clone(),
            subtitle_data: this.subtitle_data.clone(),
            subtitle_cells: this.subtitle_cells,
        };
        next.timeshift_bytes_per_second =
            this.media.timeshift_bytes_per_second().unwrap_or_default();
        if next.stream_state.active() {
            next.speed = this.media.speed();
            if let Some((phase, snapshot)) = this.media.timeline() {
                next.stream_state = std::mem::take(&mut next.stream_state).transport(phase);
                next.timeline = snapshot;
            }
        } else {
            next.timeline = Default::default();
            next.speed = Default::default();
            next.timeshift_bytes_per_second = 0.0;
        }
        let source_changed = before.recording != next.stream_state.recording().is_some()
            || before.recording_name.to_string()
                != next.stream_state.recording().map_or("", |file| file.name());
        if source_changed
            || (!next.stream_state.active() && before.live_timeline.to_string() != "null")
        {
            this.current_projection = Default::default();
            this.program_publication = Default::default();
            this.recording_program_bodies = Default::default();
            this.program_enrichment = Default::default();
            next.current_program_data = QString::from("null");
            next.program_progress = 0.0;
            next.subtitle_data = QString::default();
            next.subtitle_cells = 0;
        }
        if next.stream_state.active()
            && let Some(mut snapshot) = this.media.live_timeline()
        {
            if this.epg_enabled {
                snapshot.supplement(this.epg.revision, &mut this.program_enrichment, |data| {
                    this.epg.supplement_data(data)
                });
            } else {
                snapshot.disable_programs();
            }
            let (_, progress) = snapshot.viewing_program();
            let data = snapshot.viewing_data();
            next.program_status = QString::from(if !this.epg_enabled {
                "disabled"
            } else if next.stream_state.seeking() {
                "pending"
            } else {
                snapshot.program_status().as_str()
            });
            if this.program_publication.update(data) {
                next.current_program_data = QString::from(this.program_publication.json());
            }
            next.program_progress = progress;
            next.live_timeline = QString::from(snapshot.serialize());
        } else {
            next.live_timeline = QString::from("null");
            if next.stream_state.active() {
                if next.stream_state.seeking() {
                    next.program_status = QString::from("pending");
                } else if let Some(position) = this
                    .media
                    .playback()
                    .and_then(|playback| playback.position())
                {
                    let view = this.media.metadata(position);
                    let (data, progress) = if this.epg_enabled {
                        view.project(position.nseconds(), &mut this.recording_program_bodies)
                    } else {
                        (None, 0.)
                    };
                    if this.program_publication.update(data) {
                        next.current_program_data = QString::from(this.program_publication.json());
                    }
                    next.program_progress = progress;
                    next.program_status = QString::from(if !this.epg_enabled {
                        "disabled"
                    } else {
                        view.status.as_str()
                    });
                }
            } else {
                next.program_status = QString::from("pending");
            }
        }
        next
    }

    fn commit(self, player: &mut PlayerRust) {
        let Self {
            stream_state,
            timeline,
            speed,
            timeshift_bytes_per_second,
            live_timeline,
            current_program_data,
            program_progress,
            program_status,
            subtitle_data,
            subtitle_cells,
        } = self;
        player.stream_state = stream_state;
        player.timeline = timeline;
        player.speed = speed;
        player.timeshift_bytes_per_second = timeshift_bytes_per_second;
        player.live_timeline = live_timeline;
        player.current_program_data = current_program_data;
        player.program_progress = program_progress;
        player.program_status = program_status;
        if subtitle_data.is_empty() {
            player.subtitle_images = Default::default();
        }
        player.subtitle_data = subtitle_data;
        player.subtitle_cells = subtitle_cells;
        player.seek_preview.synchronize(
            player
                .stream_state
                .active()
                .then(|| player.media.preview_identity())
                .flatten(),
        );
        // Commit posting availability before any transport notification. Pausing
        // and seeking must disable submission without waiting for a comment poll.
        player.comment_post_available = player.commentary.posting_available(
            player
                .stream_state
                .commentary_playback(player.catalog.selected(), &player.media),
            player.network.as_ref(),
            std::time::Instant::now(),
        );
    }
}

// Each phase exclusively borrows this Player. A prepared update cannot notify,
// and committed notifications cannot be applied to another Player or replayed.
struct Prepared<'a> {
    player: Pin<&'a mut ffi::Player>,
    before: Snapshot,
    next: Values,
}
struct Committed<'a> {
    player: Pin<&'a mut ffi::Player>,
    before: Snapshot,
    after: Snapshot,
}
impl<'a> Prepared<'a> {
    fn new(mut player: Pin<&'a mut ffi::Player>, change: impl FnOnce(State) -> State) -> Self {
        let before = Snapshot::capture(&player);
        let next = Values::prepare(&mut player.as_mut().rust_mut(), &before, change);
        Self {
            player,
            before,
            next,
        }
    }
    fn commit(mut self) -> Committed<'a> {
        self.next.commit(&mut self.player.as_mut().rust_mut());
        super::data_broadcast::synchronize(&mut self.player.as_mut().rust_mut());
        let after = Snapshot::capture(&self.player);
        Committed {
            player: self.player,
            before: self.before,
            after,
        }
    }
}
impl Committed<'_> {
    fn notify(mut self) {
        self.before
            .notify_changes(&self.after, self.player.as_mut());
        if self.player.ended() && !self.before.ended {
            self.player.update_status(PlaybackStatus::Finished);
        } else if self.before.ended && self.player.media_active() {
            let name = self.player.recording_name().to_string();
            self.player.update_status(PlaybackStatus::Playing(name));
        }
    }
}

pub(super) fn apply(player: Pin<&mut ffi::Player>, change: impl FnOnce(State) -> State) {
    Prepared::new(player, change).commit().notify();
}
