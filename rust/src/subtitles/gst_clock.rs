use super::{
    SubtitleCue,
    timing::{Anchor, SubtitleUpdate, Timeline},
};
use gst::prelude::*;
use gstreamer as gst;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

type StreamKey = (String, u32);

#[derive(Default)]
struct State {
    raw_pts: HashMap<StreamKey, VecDeque<u64>>,
    video: Option<StreamKey>,
    timeline: Timeline,
}

/// Bridges tsdemux's transport PTS to the video sink's queried stream position.
/// The synchronous stats handler runs before the corresponding demuxed PES is
/// pushed. Matching them by PID and PES order also covers initial pending PES.
#[derive(Clone, Default)]
pub(crate) struct SubtitleClock(Arc<Mutex<State>>);

impl SubtitleClock {
    pub fn reset(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.raw_pts.clear();
            state.video = None;
            state.timeline.reset();
        }
    }

    pub fn push(&self, cues: Vec<SubtitleCue>) {
        if let Ok(mut state) = self.0.lock() {
            for cue in cues {
                state.timeline.push(cue);
            }
        }
    }

    pub fn poll(&self, position: Option<gst::ClockTime>) -> SubtitleUpdate {
        self.0
            .lock()
            .map(|mut state| state.timeline.poll(position.map(|time| time.nseconds())))
            .unwrap_or(SubtitleUpdate::Unchanged)
    }

    pub fn attach(&self, playbin: &gst::Bin) {
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
                    ) && let Ok(mut state) = clock.0.lock()
                    {
                        let key = (source.name().to_string(), pid);
                        // Bound streams and pending PES even for malformed input.
                        if state.raw_pts.len() < 64 || state.raw_pts.contains_key(&key) {
                            let pending = state.raw_pts.entry(key).or_default();
                            if pending.len() == 256 {
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
        playbin.connect_deep_element_added(move |_, _, element| {
            if element
                .factory()
                .is_none_or(|factory| factory.name() != "tsdemux")
            {
                return;
            }
            element.set_property("emit-stats", true);
            let removed_clock = clock.clone();
            element.connect_pad_removed(move |demux, pad| {
                let Some(pid) = pad
                    .name()
                    .rsplit('_')
                    .next()
                    .and_then(|pid| u32::from_str_radix(pid, 16).ok())
                else {
                    return;
                };
                let key = (demux.name().to_string(), pid);
                if let Ok(mut state) = removed_clock.0.lock() {
                    state.raw_pts.remove(&key);
                    if state.video.as_ref() == Some(&key) {
                        state.video = None;
                        state.timeline.reset();
                    }
                }
            });
            let clock = clock.clone();
            element.connect_pad_added(move |demux, pad| {
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
                if let Ok(mut state) = clock.0.lock() {
                    if state
                        .video
                        .as_ref()
                        .is_some_and(|current| current.0 == key.0 && current != &key)
                    {
                        return;
                    }
                    state.video = Some(key.clone());
                }
                clock.attach_video_pad(pad, key);
            });
        });
    }

    fn attach_video_pad(&self, pad: &gst::Pad, key: StreamKey) {
        let clock = self.clone();
        let segment_state = Mutex::new(None::<gst::FormattedSegment<gst::ClockTime>>);
        pad.add_probe(
            gst::PadProbeType::BUFFER
                | gst::PadProbeType::BUFFER_LIST
                | gst::PadProbeType::EVENT_DOWNSTREAM,
            move |_, info| {
                let Ok(mut segment) = segment_state.lock() else {
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
        if let Ok(mut state) = self.0.lock() {
            if state.video.is_none() {
                state.video = Some(key.clone());
            }
            if state.video.as_ref() != Some(key) {
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
                        .is_some_and(|mapped| (mapped - now).abs() < 5_000_000)
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

    struct PipelineGuard(gst::Pipeline);
    impl Drop for PipelineGuard {
        fn drop(&mut self) {
            let _ = self.0.set_state(gst::State::Null);
        }
    }

    #[test]
    fn recovers_pes_correspondence_and_resets_a_reused_pad() {
        gst::init().unwrap();
        let clock = SubtitleClock::default();
        let key = ("tsdemux-test".to_owned(), 256);
        {
            let mut state = clock.0.lock().unwrap();
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
        assert!(clock.0.lock().unwrap().raw_pts[&key].is_empty());
        clock.push(vec![SubtitleCue {
            text: "test".into(),
            clear_screen: false,
            ..SubtitleCue::clear(10080)
        }]);
        assert!(matches!(
            clock.poll(Some(gst::ClockTime::from_mseconds(2079))),
            SubtitleUpdate::Unchanged
        ));
        assert!(matches!(
            clock.poll(Some(gst::ClockTime::from_mseconds(2080))),
            SubtitleUpdate::Show(_)
        ));
        clock.reset();
        assert!(matches!(clock.poll(None), SubtitleUpdate::Clear));
        clock
            .0
            .lock()
            .unwrap()
            .raw_pts
            .insert(key.clone(), VecDeque::from([450_000]));
        clock.observe_buffer(&key, Some(&segment), &buffer);
        assert_eq!(
            clock.0.lock().unwrap().timeline.map_ticks(450_000),
            Some(2_080_000_000)
        );
    }

    #[test]
    fn maps_real_demuxed_pes_to_the_video_segment() {
        gst::init().unwrap();
        // Only demux to memory: this test needs no decoder, display, GPU or sound device.
        let data = include_bytes!("../../../tests/fixtures/subtitle-clock.ts");
        let raw_pts: Vec<i64> = data
            .chunks_exact(188)
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
                let pts = super::super::pes_pts_ms(pes);
                (pts != i64::MIN).then_some(pts)
            })
            .collect();
        assert!(raw_pts.len() >= 40);
        let pipeline = PipelineGuard(gst::Pipeline::new());
        let clock = SubtitleClock::default();
        clock.attach(pipeline.0.upcast_ref());
        let source = gst::ElementFactory::make("filesrc")
            .property(
                "location",
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../tests/fixtures/subtitle-clock.ts"
                ),
            )
            .build()
            .unwrap();
        let demux = gst::ElementFactory::make("tsdemux").build().unwrap();
        let sink = gst::ElementFactory::make("appsink")
            .property("sync", false)
            .property("async", false)
            .property("max-buffers", 1_u32)
            .build()
            .unwrap();
        pipeline.0.add_many([&source, &demux, &sink]).unwrap();
        source.link(&demux).unwrap();
        let sink_pad = sink.static_pad("sink").unwrap();
        demux.connect_pad_added(move |_, pad| {
            if pad.name().starts_with("video_") {
                pad.link(&sink_pad).unwrap();
            }
        });
        pipeline.0.set_state(gst::State::Playing).unwrap();
        let mut samples = 0;
        while let Some(sample) =
            sink.emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&5_000_000_000_u64])
        {
            let Some(pts) = sample.buffer().unwrap().pts() else {
                continue;
            };
            let stream_time = sample
                .segment()
                .unwrap()
                .downcast_ref::<gst::ClockTime>()
                .unwrap()
                .to_stream_time(pts)
                .unwrap();
            clock.push(vec![SubtitleCue {
                text: "test".into(),
                clear_screen: false,
                ..SubtitleCue::clear(raw_pts[samples])
            }]);
            assert!(
                matches!(
                    clock.poll(stream_time.checked_sub(gst::ClockTime::NSECOND)),
                    SubtitleUpdate::Unchanged
                ),
                "sample {samples} appeared early"
            );
            assert!(
                matches!(clock.poll(Some(stream_time)), SubtitleUpdate::Show(_)),
                "sample {samples} was not aligned with its video PTS"
            );
            samples += 1;
        }
        assert!(sink.property::<bool>("eos"), "demux did not reach EOS");
        assert_eq!(samples, raw_pts.len());
    }
}
