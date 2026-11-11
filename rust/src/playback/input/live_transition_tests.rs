//! Authored broadcast changes through raw playbin3 and the production TS reader.
use super::*;
use gstreamer::{self as gst, prelude::*};
use std::sync::atomic::AtomicU64;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const AUDIT_DEADLINE: Duration = Duration::from_secs(12);
const CASES: &[(&str, u64)] = &[
    ("live-transition-version.ts", 200),
    ("live-transition-pcr.ts", 200),
    ("live-transition-audio.ts", 200),
    ("live-transition-service.ts", 125),
    ("live-transition-codec.ts", 250),
    ("recording-pid-change.ts", 125),
    ("recording-clock-reset.ts", 125),
    ("recording-caption-change.ts", 1400),
];
#[derive(Debug)]
struct Audit {
    eos: bool,
    video: u64,
    audio: u64,
    position: Option<u64>,
    error: Option<String>,
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
    let mut eos = false;
    let mut error = None;
    while Instant::now() < deadline {
        if let Some(message) = bus.timed_pop(gst::ClockTime::from_mseconds(10)) {
            match message.view() {
                gst::MessageView::Eos(_) => {
                    eos = true;
                    break;
                }
                gst::MessageView::Error(failure) => {
                    error = Some(format!("{} {:?}", failure.error(), failure.debug()));
                    break;
                }
                _ => {}
            }
        }
    }
    let result = Audit {
        eos,
        video: video.load(Ordering::Relaxed),
        audio: audio.load(Ordering::Relaxed),
        position: playbin
            .query_position::<gst::ClockTime>()
            .map(|time| time.mseconds()),
        error,
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
    for &(name, minimum_video) in CASES {
        let raw = audit(name, false)?;
        let normalized = audit(name, true)?;
        assert!(
            normalized.eos,
            "{name}: normalized output did not end: {normalized:?}; raw={raw:?}"
        );
        assert!(normalized.error.is_none(), "{name}: {normalized:?}");
        assert!(
            normalized.video >= minimum_video,
            "{name}: missing post-transition video: {normalized:?}"
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
