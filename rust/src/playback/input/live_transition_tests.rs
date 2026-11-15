//! Authored broadcast changes through raw playbin3 and the production TS reader.
use super::*;
use gstreamer::{self as gst, prelude::*};
use std::sync::atomic::AtomicU64;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const AUDIT_DEADLINE: Duration = Duration::from_secs(12);
const CODEC_CHANGE_FIXTURE: &str = "live-transition-codec.ts";
pub(super) const CODEC_INITIAL_VIDEO_FRAMES: u64 = 75;
const VIDEO_FRAME_TOLERANCE: u64 = 25;
const CODEC_AUDIO_BUFFERS: u64 = 706;
const CODEC_FINAL_POSITION_MS: u64 = 15_000;
const CASES: &[(&str, u64)] = &[
    ("live-transition-version.ts", 200),
    ("live-transition-pcr.ts", 200),
    ("live-transition-audio.ts", 200),
    ("live-transition-service.ts", 125),
    (CODEC_CHANGE_FIXTURE, 250),
    ("recording-pid-change.ts", 125),
    ("recording-clock-reset.ts", 125),
    ("recording-caption-change.ts", 1400),
];
#[derive(Debug)]
enum End {
    Eos,
    Error(String),
    Deadline,
}
#[derive(Debug, Default)]
struct SourceQueueEos {
    video_received: AtomicBool,
    video_forwarded: AtomicBool,
    audio_forwarded: AtomicBool,
}
impl SourceQueueEos {
    fn observe(self: &Arc<Self>, element: &gst::Element) {
        if element
            .factory()
            .is_none_or(|factory| factory.name() != "multiqueue")
            || element
                .parent()
                .is_none_or(|parent| parent.type_().name() != "GstURISourceBin")
        {
            return;
        }
        for pad in element.pads() {
            self.attach(&pad);
        }
        let observer = self.clone();
        element.connect_pad_added(move |_, pad| observer.attach(pad));
    }
    fn attach(self: &Arc<Self>, pad: &gst::Pad) {
        let observer = self.clone();
        pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |pad, info| {
            if info
                .event()
                .is_some_and(|event| matches!(event.view(), gst::EventView::Eos(_)))
                && let Some(caps) = pad.current_caps()
                && let Some(structure) = caps.structure(0)
            {
                let flag = match (pad.direction(), structure.name().as_str()) {
                    (gst::PadDirection::Sink, name) if name.starts_with("video/") => {
                        Some(&observer.video_received)
                    }
                    (gst::PadDirection::Src, name) if name.starts_with("video/") => {
                        Some(&observer.video_forwarded)
                    }
                    (gst::PadDirection::Src, name) if name.starts_with("audio/") => {
                        Some(&observer.audio_forwarded)
                    }
                    _ => None,
                };
                if let Some(flag) = flag {
                    flag.store(true, Ordering::Release);
                }
            }
            gst::PadProbeReturn::Ok
        });
    }
}
#[derive(Debug)]
struct Audit {
    end: End,
    video: u64,
    audio: u64,
    position: Option<u64>,
    source_queue: Arc<SourceQueueEos>,
}
impl Audit {
    fn check_end(&self, name: &str, legacy_codec: bool) {
        match &self.end {
            End::Eos => {}
            End::Deadline if legacy_codec => {
                // 1.24.2 can retire the MPEG-2 pad without forwarding its queued
                // EOS. A generic timeout is not evidence of this specific defect.
                assert!(
                    self.source_queue.video_received.load(Ordering::Acquire)
                        && !self.source_queue.video_forwarded.load(Ordering::Acquire)
                        && self.source_queue.audio_forwarded.load(Ordering::Acquire)
                        && (CODEC_INITIAL_VIDEO_FRAMES - VIDEO_FRAME_TOLERANCE
                            ..=CODEC_INITIAL_VIDEO_FRAMES)
                            .contains(&self.video)
                        && self.audio == CODEC_AUDIO_BUFFERS
                        && self
                            .position
                            .is_some_and(|position| position >= CODEC_FINAL_POSITION_MS),
                    "{name}: not the 1.24.2 retired-video EOS defect: {self:?}"
                );
            }
            End::Deadline => panic!("{name}: output did not end: {self:?}"),
            End::Error(error) => panic!("{name}: decoder error: {error}; {self:?}"),
        }
    }
}
struct Running {
    playbin: gst::Element,
    input: Option<Input>,
}
impl Drop for Running {
    fn drop(&mut self) {
        if let Some(input) = &self.input {
            input.suspend(true);
        }
        if let Err(error) = self.playbin.set_state(gst::State::Null) {
            tracing::error!(%error, "Could not stop live transition audit");
        }
    }
}
fn audit(name: &str, normalized: bool) -> TestResult<Audit> {
    let video = Arc::new(AtomicU64::new(0));
    let audio = Arc::new(AtomicU64::new(0));
    let sink = |counter: Arc<AtomicU64>| -> Result<gst::Element, gst::glib::BoolError> {
        let sink = gst::ElementFactory::make("fakesink")
            .property("sync", false)
            .property("signal-handoffs", true)
            .build()?;
        sink.connect("handoff", false, move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
            None
        });
        Ok(sink)
    };
    let playbin = gst::ElementFactory::make("playbin3")
        .property("video-sink", sink(video.clone())?)
        .property("audio-sink", sink(audio.clone())?)
        .build()?;
    let source_queue = Arc::new(SourceQueueEos::default());
    let observer = source_queue.clone();
    playbin.connect("element-setup", false, move |values| {
        observer.observe(
            &values[1]
                .get::<gst::Element>()
                .expect("element-setup element"),
        );
        None
    });
    // Explicit, hardware-free integration mode; production decoder policy is untouched.
    playbin.set_property_from_str("flags", "video+audio+soft-volume+force-sw-decoders");
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures")
        .join(name);
    let input = if normalized {
        Some(Input::file(&playbin, &path, 1, false)?)
    } else {
        None
    };
    let _running = Running {
        playbin: playbin.clone(),
        input,
    };
    playbin.set_property(
        "uri",
        if normalized {
            "appsrc://".to_owned()
        } else {
            url::Url::from_file_path(&path)
                .map_err(|_| "fixture URI")?
                .to_string()
        },
    );
    playbin.set_state(gst::State::Playing)?;
    let bus = playbin.bus().ok_or("bus")?;
    let deadline = Instant::now() + AUDIT_DEADLINE;
    let mut end = End::Deadline;
    while Instant::now() < deadline {
        if let Some(message) = bus.timed_pop(gst::ClockTime::from_mseconds(10)) {
            match message.view() {
                gst::MessageView::Eos(_) => {
                    end = End::Eos;
                    break;
                }
                gst::MessageView::Error(failure) => {
                    end = End::Error(format!("{} {:?}", failure.error(), failure.debug()));
                    break;
                }
                _ => {}
            }
        }
    }
    let result = Audit {
        end,
        video: video.load(Ordering::Relaxed),
        audio: audio.load(Ordering::Relaxed),
        position: playbin
            .query_position::<gst::ClockTime>()
            .map(|time| time.mseconds()),
        source_queue,
    };
    eprintln!(
        "LIVE_TRANSITION {name} normalized={normalized} {result:?} GStreamer={}",
        gst::version_string()
    );
    Ok(result)
}

#[test]
fn live_transition_comparison_preserves_decoded_output_after_normalization() -> TestResult {
    gst::init()?;
    let demux_version = gst::Plugin::load_by_name("mpegtsdemux")?.version();
    eprintln!("LIVE_TRANSITION_BACKEND mpegtsdemux={demux_version}");
    for &(name, minimum_video) in CASES {
        let raw = audit(name, false)?;
        let normalized = audit(name, true)?;
        let legacy_codec = name == CODEC_CHANGE_FIXTURE && demux_version == "1.24.2";
        let minimum_video = if legacy_codec {
            // This demux version loses the same-PID codec update even for raw TS.
            // Require normalization to preserve its native output, rather than
            // promising that an input filter repairs downstream codec handling.
            let minimum_initial_video = CODEC_INITIAL_VIDEO_FRAMES - VIDEO_FRAME_TOLERANCE;
            raw.check_end(name, legacy_codec);
            assert!(
                raw.video >= minimum_initial_video,
                "{name}: missing baseline video: {raw:?}"
            );
            raw.video
                .saturating_sub(VIDEO_FRAME_TOLERANCE)
                .max(minimum_initial_video)
        } else {
            minimum_video
        };
        normalized.check_end(name, legacy_codec);
        assert!(
            normalized.video >= minimum_video,
            "{name}: expected at least {minimum_video} video buffers: {normalized:?}; raw={raw:?}"
        );
        assert!(
            normalized.audio > 100,
            "{name}: missing audio: {normalized:?}"
        );
        assert!(normalized.position.is_some(), "{name}: missing output time");
    }
    Ok(())
}

#[test]
fn authored_live_transitions_update_the_input_diagnostic_without_counting_null_packets()
-> TestResult {
    for &(name, _) in CASES {
        let bytes = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../tests/fixtures")
                .join(name),
        )?;
        let mut index = Index::new(1, false);
        for (number, packet) in bytes.as_chunks::<TS_PACKET_SIZE>().0.iter().enumerate() {
            index.receive_packet((number * TS_PACKET_SIZE) as u64, packet);
        }
        let before = index.recovery_sample(1);
        assert!(before.video_input > 100, "{name}");
        assert!(before.audio_input > 50, "{name}");
        assert!(index.recovery_diagnostic().contains("service=1"));
        let mut null_packet = [0xff; TS_PACKET_SIZE];
        null_packet[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
        index.receive_packet(bytes.len() as u64, &null_packet);
        let after = index.recovery_sample(1);
        assert!(after.received_bytes > before.received_bytes);
        assert_eq!(
            (after.video_input, after.audio_input),
            (before.video_input, before.audio_input)
        );
    }
    Ok(())
}

#[test]
fn generated_tables_cover_version_pcr_audio_service_and_codec_changes() -> TestResult {
    use crate::transport::{
        Sections,
        wire::{Pid, PsiSection, TransportPacket},
    };
    use std::collections::{BTreeMap, BTreeSet};
    for name in ["version", "pcr", "audio", "service", "codec"] {
        let bytes = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../tests/fixtures/live-transition-{name}.ts")),
        )?;
        assert!(bytes.len().is_multiple_of(TS_PACKET_SIZE));
        let mut sections = BTreeMap::<Pid, Sections>::new();
        let mut maps = BTreeSet::new();
        for bytes in bytes.as_chunks::<TS_PACKET_SIZE>().0 {
            let packet = TransportPacket::parse(bytes)?;
            if packet.pid != Pid(0x20) && packet.pid != Pid::PAT {
                continue;
            }
            for data in sections
                .entry(packet.pid)
                .or_default()
                .push(packet.start, packet.payload)
            {
                let section = PsiSection::parse(&data)?;
                if packet.pid == Pid::PAT {
                    section.pat_programs()?;
                    continue;
                }
                let map = section.0.program_map()?;
                maps.insert((
                    map.service,
                    section.version,
                    map.pcr_pid.0,
                    map.streams
                        .iter()
                        .map(|stream| (stream.stream_type, stream.pid.0))
                        .collect::<Vec<_>>(),
                ));
            }
        }
        let original = (1, 0, 0x41, vec![(2, 0x41), (0x0f, 0x42)]);
        assert!(maps.contains(&original), "{name}: {maps:?}");
        let changed = match name {
            "version" => (1, 1, 0x41, original.3.clone()),
            "pcr" => (1, 1, 0x1fe, original.3.clone()),
            "audio" => (1, 1, 0x41, vec![(2, 0x41), (0x0f, 0x42), (0x0f, 0x43)]),
            "service" => (2, 1, 0x41, original.3.clone()),
            "codec" => (1, 1, 0x41, vec![(0x1b, 0x41), (0x0f, 0x42)]),
            _ => unreachable!(),
        };
        assert!(maps.contains(&changed), "{name}: {maps:?}");
        if name != "codec" {
            assert!(
                maps.contains(&(1, 2, 0x41, original.3)),
                "{name}: original configuration did not return"
            );
        }
    }
    Ok(())
}
