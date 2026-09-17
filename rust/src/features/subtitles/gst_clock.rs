use super::{
    Error, SubtitleCue,
    timing::{Anchor, SubtitleUpdate, Timeline},
};
use crate::features::subscriptions::Subscriptions;
use gst::prelude::*;
use gstreamer as gst;
use std::collections::{HashMap, VecDeque};
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicBool, Ordering},
};

type StreamKey = (String, u32);
type Segment = Option<gst::FormattedSegment<gst::ClockTime>>;
const MAX_TIMESTAMP_STREAMS: usize = 64;
const MAX_PENDING_PES: usize = 256;
const PTS_MATCH_TOLERANCE_NS: i128 = 5_000_000;
const CLOCK_RESET_THRESHOLD_NS: i128 = 500_000_000;

#[derive(Default)]
struct State {
    disabled: bool,
    raw_pts: HashMap<StreamKey, VecDeque<u64>>,
    video: Option<StreamKey>,
    timeline: Timeline,
    programs: crate::transport::programs::Timeline,
}

/// Bridges tsdemux's transport PTS to the video sink's queried stream position.
/// The synchronous stats handler runs before the corresponding demuxed PES is
/// pushed. Matching them by PID and PES order also covers initial pending PES.
#[derive(Clone, Default)]
pub(crate) struct SubtitleClock(Arc<Shared>);

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    segment_failed: AtomicBool,
}

impl SubtitleClock {
    pub fn check(&self) -> Result<(), Error> {
        // These flags publish no associated data. Never reuse poisoned clock
        // state or a segment whose mapping may have been partially updated.
        if self.0.state.is_poisoned() || self.0.segment_failed.load(Ordering::Relaxed) {
            Err(Error::ClockPoisoned)
        } else {
            Ok(())
        }
    }

    fn state(&self) -> Result<MutexGuard<'_, State>, Error> {
        self.check()?;
        self.0.state.lock().map_err(|_| Error::ClockPoisoned)
    }

    fn segment<'a>(&self, segment: &'a Mutex<Segment>) -> Result<MutexGuard<'a, Segment>, Error> {
        self.check()?;
        segment.lock().map_err(|_| {
            self.0.segment_failed.store(true, Ordering::Relaxed);
            Error::ClockPoisoned
        })
    }

    pub fn pending_count(&self) -> Option<usize> {
        self.check().ok()?;
        self.0
            .state
            .try_lock()
            .ok()
            .map(|state| state.timeline.pending_count())
    }

    pub fn reset(&self) {
        if let Ok(mut state) = self.state() {
            // Release old stream keys and queue allocations at channel boundaries.
            let disabled = state.disabled;
            *state = State {
                disabled,
                ..State::default()
            };
            state.timeline.reset();
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        if let Ok(mut state) = self.state()
            && state.disabled == enabled
        {
            *state = State {
                disabled: !enabled,
                ..State::default()
            };
            state.timeline.reset();
        }
    }

    pub fn push(&self, cues: Vec<SubtitleCue>) {
        if let Ok(mut state) = self.state() {
            if state.disabled {
                return;
            }
            for cue in cues {
                state.timeline.push(cue);
            }
        }
    }

    pub fn push_programs(&self, observation: crate::transport::programs::Observation) {
        if let Ok(mut state) = self.state() {
            state.programs.push(observation);
        }
    }
    pub fn clear_captions(&self) {
        if let Ok(mut state) = self.state() {
            state.timeline.clear_captions();
        }
    }

    pub fn poll(&self, position: Option<gst::ClockTime>) -> Result<SubtitleUpdate, Error> {
        Ok(self
            .state()?
            .timeline
            .poll(position.map(|time| time.nseconds())))
    }

    pub fn attach(&self, playbin: &gst::Bin) -> Subscriptions {
        let subscriptions = Subscriptions::default();
        let registrations = subscriptions.clone();
        let clock = self.clone();
        if let Some(bus) = playbin.bus() {
            bus.set_sync_handler(move |_, message| {
                if let gst::MessageView::Element(element) = message.view()
                    && let Some(structure) = element.structure()
                    && structure.name() == "tsdemux"
                {
                    if let (Some(source), Ok(pid), Ok(pts)) = (
                        element.src(),
                        structure.get::<u32>("pid"),
                        structure.get::<u64>("pts"),
                    ) && let Ok(mut state) = clock.state()
                    {
                        if state.disabled {
                            return gst::BusSyncReply::Drop;
                        }
                        let key = (source.name().to_string(), pid);
                        // Bound streams and pending PES even for malformed input.
                        if state.raw_pts.len() < MAX_TIMESTAMP_STREAMS
                            || state.raw_pts.contains_key(&key)
                        {
                            let pending = state.raw_pts.entry(key).or_default();
                            if pending.len() == MAX_PENDING_PES {
                                pending.pop_front();
                            }
                            pending.push_back(pts);
                        }
                    }
                    // Statistics are consumed here, not accumulated in the GUI bus.
                    return gst::BusSyncReply::Drop;
                }
                gst::BusSyncReply::Pass
            });
        }
        let clock = self.clone();
        let deep_id = playbin.connect_deep_element_added(move |_, _, element| {
            if element
                .factory()
                .is_none_or(|factory| factory.name() != "tsdemux")
            {
                return;
            }
            element.set_property("emit-stats", true);
            registrations.stats(element);
            let removed_clock = clock.clone();
            let removed = element.connect_pad_removed(move |demux, pad| {
                let Some(pid) = pad
                    .name()
                    .rsplit('_')
                    .next()
                    .and_then(|pid| u32::from_str_radix(pid, 16).ok())
                else {
                    return;
                };
                let key = (demux.name().to_string(), pid);
                if let Ok(mut state) = removed_clock.state() {
                    state.raw_pts.remove(&key);
                    if state.video.as_ref() == Some(&key) {
                        state.video = None;
                        state.timeline.reset();
                    }
                }
            });
            registrations.signal(element, removed);
            let clock = clock.clone();
            let pads = registrations.clone();
            let added = element.connect_pad_added(move |demux, pad| {
                if !pad.name().starts_with("video_") {
                    return;
                }
                let Some(pid) = pad
                    .name()
                    .rsplit('_')
                    .next()
                    .and_then(|pid| u32::from_str_radix(pid, 16).ok())
                else {
                    return;
                };
                let key = (demux.name().to_string(), pid);
                if let Ok(mut state) = clock.state() {
                    if state.video.is_none() {
                        state.video = Some(key.clone());
                    }
                } else {
                    return;
                }
                // tsdemux may add the replacement before removing the old pad.
                // Observe every candidate now; observe_buffer keeps the current
                // video authoritative until removal makes a replacement eligible.
                clock.attach_video_pad(pad, key, &pads);
            });
            registrations.signal(element, added);
        });
        subscriptions.signal(playbin, deep_id);
        subscriptions
    }

    fn attach_video_pad(&self, pad: &gst::Pad, key: StreamKey, subscriptions: &Subscriptions) {
        let clock = self.clone();
        let segment_state = Mutex::new(None::<gst::FormattedSegment<gst::ClockTime>>);
        let id = pad.add_probe(
            gst::PadProbeType::BUFFER
                | gst::PadProbeType::BUFFER_LIST
                | gst::PadProbeType::EVENT_DOWNSTREAM
                | gst::PadProbeType::EVENT_FLUSH,
            move |_, info| {
                let Ok(mut segment) = clock.segment(&segment_state) else {
                    return gst::PadProbeReturn::Ok;
                };
                if let Some(event) = info.event() {
                    match event.view() {
                        gst::EventView::Segment(event) => {
                            *segment = event.segment().downcast_ref::<gst::ClockTime>().cloned();
                        }
                        gst::EventView::StreamStart(_) => {
                            *segment = None;
                        }
                        gst::EventView::FlushStart(_) => {
                            *segment = None;
                            clock.reset();
                        }
                        _ => {}
                    }
                }
                if let Some(buffer) = info.buffer() {
                    clock.observe_buffer(&key, segment.as_ref(), buffer);
                } else if let Some(list) = info.buffer_list() {
                    for buffer in list.iter() {
                        clock.observe_buffer(&key, segment.as_ref(), buffer);
                    }
                }
                gst::PadProbeReturn::Ok
            },
        );
        subscriptions.probe(pad, id);
    }

    fn observe_buffer(
        &self,
        key: &StreamKey,
        segment: Option<&gst::FormattedSegment<gst::ClockTime>>,
        buffer: &gst::BufferRef,
    ) {
        let Some(pts) = buffer.pts() else {
            return;
        };
        if let Ok(mut state) = self.state() {
            if state.disabled {
                return;
            }
            if state.video.is_none() {
                state.video = Some(key.clone());
            }
            if state.video.as_ref() != Some(key) {
                // This PES was observed, even though its video is not selected.
                // Keeping its timestamp would shift the replacement's anchor.
                state.raw_pts.get_mut(key).and_then(VecDeque::pop_front);
                return;
            }
            let Some(stream_time) = segment.and_then(|segment| segment.to_stream_time(pts)) else {
                // Consume the corresponding timestamp even for a clipped PES.
                state.raw_pts.get_mut(key).and_then(VecDeque::pop_front);
                return;
            };
            let now = i128::from(stream_time.nseconds());
            // A damaged PES can be dropped by tsdemux. Recover correspondence
            // using the established mapping instead of shifting every later cue.
            let matched = state.raw_pts.get(key).and_then(|pending| {
                pending.iter().position(|raw| {
                    state
                        .timeline
                        .map_ticks(*raw)
                        .is_some_and(|mapped| (mapped - now).abs() < PTS_MATCH_TOLERANCE_NS)
                })
            });
            let first = state
                .raw_pts
                .get(key)
                .and_then(|pending| pending.front())
                .copied();
            let has_anchor = first
                .and_then(|pts| state.timeline.map_ticks(pts))
                .is_some();
            let index = match matched {
                Some(index) => index,
                None if !has_anchor || buffer.flags().contains(gst::BufferFlags::DISCONT) => 0,
                None => return,
            };
            let Some(pending) = state.raw_pts.get_mut(key) else {
                return;
            };
            for _ in 0..index {
                pending.pop_front();
            }
            if let Some(raw_pts) = pending.pop_front() {
                if state
                    .timeline
                    .map_ticks(raw_pts)
                    .is_some_and(|old| (old - now).abs() > CLOCK_RESET_THRESHOLD_NS)
                {
                    state.programs = Default::default();
                }
                state.timeline.anchor(Anchor {
                    pts: raw_pts,
                    stream_ns: stream_time.nseconds(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // In tests, unwrap/expect assert successful setup or an expected result.
    // Failures intentionally fail the test; they are not assumed impossible IO.
    use super::*;

    #[test]
    fn poisoned_clock_is_reported_and_never_reenabled() {
        let clock = SubtitleClock::default();
        clock.push(vec![SubtitleCue::clear(100)]);
        let other = clock.clone();
        assert!(
            std::thread::spawn(move || {
                let _guard = other.0.state.lock().unwrap();
                panic!("inject clock poison");
            })
            .join()
            .is_err()
        );
        assert!(matches!(clock.poll(None), Err(Error::ClockPoisoned)));
        clock.set_enabled(false);
        clock.set_enabled(true);
        clock.push(vec![SubtitleCue::clear(200)]);
        assert!(matches!(clock.check(), Err(Error::ClockPoisoned)));
        assert_eq!(clock.pending_count(), None);
        let ingest = super::super::ingest::Ingest::new(
            super::super::transport::TransportParser::new(false),
            clock,
        );
        assert!(matches!(ingest.check(), Err(Error::ClockPoisoned)));
        ingest.consume(std::iter::empty());
        assert_eq!(ingest.decoded(), 0);
        assert!(SubtitleClock::default().check().is_ok());
    }

    #[test]
    fn segment_poison_invalidates_the_whole_clock_generation() {
        let clock = SubtitleClock::default();
        let segment = Arc::new(Mutex::new(None));
        let other = segment.clone();
        assert!(
            std::thread::spawn(move || {
                let _guard = other.lock().unwrap();
                panic!("inject segment poison");
            })
            .join()
            .is_err()
        );
        assert!(matches!(clock.segment(&segment), Err(Error::ClockPoisoned)));
        assert!(matches!(clock.poll(None), Err(Error::ClockPoisoned)));
        // A new pad must not reset a failure within the old playback generation.
        assert!(matches!(
            clock.segment(&Mutex::new(None)),
            Err(Error::ClockPoisoned)
        ));
        assert!(matches!(clock.state(), Err(Error::ClockPoisoned)));
    }

    struct PipelineGuard(gst::Pipeline);
    impl Drop for PipelineGuard {
        fn drop(&mut self) {
            let _ = self.0.set_state(gst::State::Null);
        }
    }

    #[test]
    fn replacement_video_added_before_old_removal_keeps_subtitles_synchronized() {
        gst::init().unwrap();
        const OLD_PID: u32 = 0x41;
        const NEW_PID: u32 = 0x141;
        const PTS_TICKS_PER_MS: u64 = 90;
        const FIRST_PTS_MS: u64 = 1000;
        const NEXT_PTS_MS: u64 = 1040;
        const VIDEO_POSITION_MS: u64 = 3040;
        let pipeline = gst::Pipeline::new();
        let clock = SubtitleClock::default();
        let scope = clock.attach(pipeline.upcast_ref());
        let demux = gst::ElementFactory::make("tsdemux").build().unwrap();
        pipeline.add(&demux).unwrap();
        let add_video = |generation, pid| {
            let pad = gst::Pad::builder(gst::PadDirection::Src)
                .name(format!("video_{generation}_{pid:04x}"))
                .build();
            pad.set_active(true).unwrap();
            demux.add_pad(&pad).unwrap();
            pad.push_event(gst::event::StreamStart::new(pad.name().as_str()));
            pad.push_event(gst::event::Segment::new(&gst::FormattedSegment::<
                gst::ClockTime,
            >::new()));
            pad
        };
        let old = add_video(0, OLD_PID);
        let new = add_video(1, NEW_PID);
        let old_key = (demux.name().to_string(), OLD_PID);
        let new_key = (demux.name().to_string(), NEW_PID);
        let push_video = |pts_ms, position_ms| {
            clock
                .state()
                .unwrap()
                .raw_pts
                .entry(new_key.clone())
                .or_default()
                .push_back(pts_ms * PTS_TICKS_PER_MS);
            let mut buffer = gst::Buffer::new();
            buffer
                .get_mut()
                .unwrap()
                .set_pts(gst::ClockTime::from_mseconds(position_ms));
            // Source pad probes run without downstream decoding or hardware.
            assert_eq!(new.push(buffer), Err(gst::FlowError::NotLinked));
        };
        push_video(FIRST_PTS_MS, VIDEO_POSITION_MS);
        assert_eq!(clock.state().unwrap().video.as_ref(), Some(&old_key));
        demux.remove_pad(&old).unwrap();
        push_video(NEXT_PTS_MS, VIDEO_POSITION_MS);
        assert_eq!(clock.state().unwrap().video.as_ref(), Some(&new_key));
        clock.poll(None).unwrap(); // Consume the clear from removing the old pad.
        clock.push(vec![SubtitleCue {
            text: "replacement video".into(),
            clear_screen: false,
            ..SubtitleCue::clear(NEXT_PTS_MS as i64)
        }]);
        assert!(matches!(
            clock
                .poll(Some(gst::ClockTime::from_mseconds(VIDEO_POSITION_MS - 1)))
                .unwrap(),
            SubtitleUpdate::Unchanged
        ));
        assert!(matches!(
            clock
                .poll(Some(gst::ClockTime::from_mseconds(VIDEO_POSITION_MS)))
                .unwrap(),
            SubtitleUpdate::Show(_)
        ));
        scope.close();
        pipeline.bus().unwrap().unset_sync_handler();
        demux.remove_pad(&new).unwrap();
    }

    #[test]
    fn demux_timestamp_reset_clears_old_captions_and_reanchors_new_ones() {
        gst::init().unwrap();
        let clock = SubtitleClock::default();
        let key = ("tsdemux-reset".to_owned(), 0x41);
        {
            let mut state = clock.state().unwrap();
            state.video = Some(key.clone());
            state.timeline.anchor(Anchor {
                pts: 900_000,
                stream_ns: 2_000_000_000,
            });
            state
                .raw_pts
                .insert(key.clone(), VecDeque::from([90_000, 93_600]));
        }
        clock.push(vec![SubtitleCue {
            text: "old future screen".into(),
            clear_screen: false,
            ..SubtitleCue::clear(11_000)
        }]);
        let segment = gst::FormattedSegment::<gst::ClockTime>::new();
        let mut buffer = gst::Buffer::new();
        buffer
            .get_mut()
            .unwrap()
            .set_pts(gst::ClockTime::from_mseconds(3000));
        buffer
            .get_mut()
            .unwrap()
            .set_flags(gst::BufferFlags::DISCONT);
        clock.observe_buffer(&key, Some(&segment), &buffer);
        assert_eq!(clock.pending_count(), Some(0));
        assert!(matches!(clock.poll(None).unwrap(), SubtitleUpdate::Clear));
        clock.push(vec![SubtitleCue {
            text: "new epoch".into(),
            clear_screen: false,
            ..SubtitleCue::clear(1040)
        }]);
        buffer
            .get_mut()
            .unwrap()
            .unset_flags(gst::BufferFlags::DISCONT);
        buffer
            .get_mut()
            .unwrap()
            .set_pts(gst::ClockTime::from_mseconds(3040));
        clock.observe_buffer(&key, Some(&segment), &buffer);
        assert!(matches!(
            clock
                .poll(Some(gst::ClockTime::from_mseconds(3039)))
                .unwrap(),
            SubtitleUpdate::Unchanged
        ));
        let SubtitleUpdate::Show(cue) = clock
            .poll(Some(gst::ClockTime::from_mseconds(3040)))
            .unwrap()
        else {
            panic!("new subtitle did not recover after PTS reset");
        };
        assert_eq!(cue.text, "new epoch");
        assert!(matches!(
            clock
                .poll(Some(gst::ClockTime::from_mseconds(5000)))
                .unwrap(),
            SubtitleUpdate::Unchanged
        ));
    }

    #[test]
    fn detach_releases_clock_captures_and_disables_demux_statistics() {
        gst::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let clock = SubtitleClock::default();
        let weak = Arc::downgrade(&clock.0);
        let scope = clock.attach(pipeline.upcast_ref());
        let demux = gst::ElementFactory::make("tsdemux").build().unwrap();
        pipeline.add(&demux).unwrap();
        assert!(demux.property::<bool>("emit-stats"));
        drop(clock);
        assert!(weak.upgrade().is_some());
        scope.close();
        pipeline.bus().unwrap().unset_sync_handler();
        assert!(!demux.property::<bool>("emit-stats"));
        assert!(weak.upgrade().is_none());
        assert_eq!(scope.count(), 0);
    }

    #[test]
    fn disabling_releases_clock_state_and_reenable_reanchors_existing_stream() {
        gst::init().unwrap();
        let clock = SubtitleClock::default();
        let key = ("tsdemux-test".to_owned(), 256);
        clock.push(vec![SubtitleCue::clear(100)]);
        clock
            .0
            .state
            .lock()
            .unwrap()
            .raw_pts
            .insert(key.clone(), VecDeque::from([9000]));
        clock.set_enabled(false);
        assert!(clock.0.state.lock().unwrap().raw_pts.is_empty());
        assert_eq!(clock.pending_count(), Some(0));
        clock.push(vec![SubtitleCue::clear(100)]);
        assert_eq!(clock.pending_count(), Some(0));
        clock.reset();
        assert!(clock.0.state.lock().unwrap().disabled);
        clock.set_enabled(true);
        assert!(matches!(clock.poll(None).unwrap(), SubtitleUpdate::Clear));
        clock
            .0
            .state
            .lock()
            .unwrap()
            .raw_pts
            .insert(key.clone(), VecDeque::from([9000]));
        let segment = gst::FormattedSegment::<gst::ClockTime>::new();
        let mut buffer = gst::Buffer::new();
        buffer
            .get_mut()
            .unwrap()
            .set_pts(gst::ClockTime::from_mseconds(500));
        clock.observe_buffer(&key, Some(&segment), &buffer);
        assert_eq!(
            clock.0.state.lock().unwrap().timeline.map_ticks(9000),
            Some(500_000_000)
        );
        clock.push(vec![SubtitleCue {
            text: "new".into(),
            clear_screen: false,
            ..SubtitleCue::clear(100)
        }]);
        assert!(matches!(
            clock
                .poll(Some(gst::ClockTime::from_mseconds(500)))
                .unwrap(),
            SubtitleUpdate::Show(_)
        ));
    }

    #[test]
    fn recovers_pes_correspondence_and_resets_a_reused_pad() {
        gst::init().unwrap();
        let clock = SubtitleClock::default();
        let key = ("tsdemux-test".to_owned(), 256);
        {
            let mut state = clock.0.state.lock().unwrap();
            state.video = Some(key.clone());
            state.timeline.anchor(Anchor {
                pts: 900_000,
                stream_ns: 2_000_000_000,
            });
            state
                .raw_pts
                .insert(key.clone(), VecDeque::from([903_600, 907_200]));
        }
        let segment = gst::FormattedSegment::<gst::ClockTime>::new();
        let mut buffer = gst::Buffer::new();
        buffer
            .get_mut()
            .unwrap()
            .set_pts(gst::ClockTime::from_mseconds(2080));
        // The PES at 10.04s was lost; the next output belongs to 10.08s.
        clock.observe_buffer(&key, Some(&segment), &buffer);
        assert!(clock.0.state.lock().unwrap().raw_pts[&key].is_empty());
        clock.push(vec![SubtitleCue {
            text: "test".into(),
            clear_screen: false,
            ..SubtitleCue::clear(10080)
        }]);
        assert!(matches!(
            clock
                .poll(Some(gst::ClockTime::from_mseconds(2079)))
                .unwrap(),
            SubtitleUpdate::Unchanged
        ));
        assert!(matches!(
            clock
                .poll(Some(gst::ClockTime::from_mseconds(2080)))
                .unwrap(),
            SubtitleUpdate::Show(_)
        ));
        clock.reset();
        assert!(matches!(clock.poll(None).unwrap(), SubtitleUpdate::Clear));
        clock
            .0
            .state
            .lock()
            .unwrap()
            .raw_pts
            .insert(key.clone(), VecDeque::from([450_000]));
        clock.observe_buffer(&key, Some(&segment), &buffer);
        assert_eq!(
            clock.0.state.lock().unwrap().timeline.map_ticks(450_000),
            Some(2_080_000_000)
        );
    }

    #[test]
    fn maps_real_demuxed_pes_to_the_video_segment() -> Result<(), Box<dyn std::error::Error>> {
        // Direct stderr bypasses libtest capture if cleanup itself blocks. Opt-in
        // tracing keeps ordinary test output quiet while preserving the last stage.
        let trace = |stage: &str| -> std::io::Result<()> {
            if std::env::var_os("SUBTITLE_TEST_TRACE").is_some() {
                use std::io::Write;
                writeln!(std::io::stderr(), "SUBTITLE_CLOCK_TEST {stage}")?;
            }
            Ok(())
        };
        gst::init()?;
        // Only demux to memory: this test needs no decoder, display, GPU or sound device.
        let data = include_bytes!("../../../../tests/fixtures/subtitle-clock.ts");
        let raw_pts: Vec<i64> = data
            .as_chunks::<188>()
            .0
            .iter()
            .filter_map(|packet| {
                if packet[1] & 0x40 == 0 || packet[3] & 0x10 == 0 {
                    return None;
                }
                let offset = 4 + if packet[3] & 0x20 != 0 {
                    1 + packet[4] as usize
                } else {
                    0
                };
                let pes = packet.get(offset..)?;
                if !pes.starts_with(&[0, 0, 1, 0xe0]) {
                    return None;
                }
                let pts = super::super::pes::pes_pts_ms(pes);
                (pts != i64::MIN).then_some(pts)
            })
            .collect();
        assert!(raw_pts.len() >= 40);
        let pipeline = PipelineGuard(gst::Pipeline::new());
        let clock = SubtitleClock::default();
        let subscriptions = clock.attach(pipeline.0.upcast_ref());
        let source = gst::ElementFactory::make("filesrc")
            .property(
                "location",
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../tests/fixtures/subtitle-clock.ts"
                ),
            )
            .build()?;
        let demux = gst::ElementFactory::make("tsdemux").build()?;
        let sink = gst::ElementFactory::make("appsink")
            .property("sync", false)
            .property("async", false)
            .property("max-buffers", 1_u32)
            .build()?;
        pipeline.0.add_many([&source, &demux, &sink])?;
        source.link(&demux)?;
        let sink_pad = sink.static_pad("sink").ok_or("appsink pad missing")?;
        let (link_tx, link_rx) = std::sync::mpsc::sync_channel(1);
        demux.connect_pad_added(move |_, pad| {
            if pad.name().starts_with("video_") {
                let _ = link_tx.try_send(pad.link(&sink_pad));
            }
        });
        trace("starting")?;
        pipeline.0.set_state(gst::State::Playing)?;
        link_rx.recv_timeout(std::time::Duration::from_secs(5))??;
        trace("pulling samples")?;
        let mut samples = 0;
        while let Some(sample) =
            sink.emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&5_000_000_000_u64])
        {
            let Some(pts) = sample.buffer().ok_or("sample buffer missing")?.pts() else {
                continue;
            };
            let stream_time = sample
                .segment()
                .ok_or("sample segment missing")?
                .downcast_ref::<gst::ClockTime>()
                .ok_or("sample segment is not time-based")?
                .to_stream_time(pts)
                .ok_or("PTS is outside sample segment")?;
            clock.push(vec![SubtitleCue {
                text: "test".into(),
                clear_screen: false,
                ..SubtitleCue::clear(*raw_pts.get(samples).ok_or("unexpected extra sample")?)
            }]);
            assert!(
                matches!(
                    clock.poll(stream_time.checked_sub(gst::ClockTime::NSECOND))?,
                    SubtitleUpdate::Unchanged
                ),
                "sample {samples} appeared early"
            );
            assert!(
                matches!(clock.poll(Some(stream_time))?, SubtitleUpdate::Show(_)),
                "sample {samples} was not aligned with its video PTS"
            );
            samples += 1;
        }
        trace("samples complete")?;
        assert!(sink.property::<bool>("eos"), "demux did not reach EOS");
        assert_eq!(samples, raw_pts.len());
        // Match Session teardown: join streaming tasks before detaching callbacks.
        trace("stopping to READY")?;
        pipeline.0.set_state(gst::State::Ready)?;
        trace("detaching callbacks")?;
        subscriptions.close();
        pipeline
            .0
            .bus()
            .ok_or("pipeline bus missing")?
            .unset_sync_handler();
        clock.set_enabled(false);
        assert_eq!(subscriptions.count(), 0);
        trace("stopping to NULL")?;
        pipeline.0.set_state(gst::State::Null)?;
        trace("complete")?;
        Ok(())
    }
}
