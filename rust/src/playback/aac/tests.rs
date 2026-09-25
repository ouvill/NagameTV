//! Explicit CPU-only tests: synthetic parser input and software decoding of TS
//! fixtures into memory. No display, GPU, or audio device is opened.
use super::*;
use std::sync::{Arc, Mutex};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const SMALL_FRAME_BYTES: usize = 32;
const LARGE_FRAME_BYTES: usize = 160;
const SAMPLE_RATE: u64 = 48_000;
const SAMPLES_PER_FRAME: u64 = 1024;
// PES timestamps use 90 kHz ticks; allow rounding within one 48 kHz sample.
const TIMESTAMP_ROUNDING: gst::ClockTime =
    gst::ClockTime::from_nseconds(gst::ClockTime::SECOND.nseconds().div_ceil(SAMPLE_RATE));

fn adts(length: usize, mpeg2: bool, crc: bool) -> Vec<u8> {
    let mut bytes = vec![0x5a; length];
    // AAC-LC, 48 kHz, stereo, one raw data block; payload need not be decoded.
    bytes[..ADTS_HEADER_BYTES].copy_from_slice(&[
        0xff,
        0xf0 | u8::from(mpeg2) << 3 | u8::from(!crc),
        0x4c,
        0x80 | ((length >> 11) as u8 & LENGTH_HIGH_MASK),
        (length >> 3) as u8,
        ((length as u8 & 7) << 5) | 0x1f,
        0xfc,
    ]);
    bytes
}

struct Parser {
    pipeline: gst::Pipeline,
    input: gst::Pad,
    output: Arc<Mutex<Vec<gst::Buffer>>>,
}

impl Parser {
    fn new(mpeg2: bool) -> Result<Self, Box<dyn std::error::Error>> {
        gst::init()?;
        register()?;
        let parser = gst::ElementFactory::make(FACTORY).build()?;
        let sink = gst::ElementFactory::make("fakesink")
            .property("async", false)
            .property("sync", false)
            .property("signal-handoffs", true)
            .build()?;
        let output = Arc::new(Mutex::new(Vec::new()));
        let captured = output.clone();
        sink.connect("handoff", false, move |values| {
            let buffer = values[1].get::<gst::Buffer>().expect("handoff buffer");
            captured.lock().expect("test output mutex").push(buffer);
            None
        });
        let pipeline = gst::Pipeline::new();
        pipeline.add_many([&parser, &sink])?;
        parser.link(&sink)?;
        let input = parser.static_pad("sink").ok_or("parser sink")?;
        let test = Self {
            pipeline,
            input,
            output,
        };
        test.pipeline.set_state(gst::State::Playing)?;
        assert!(
            test.input
                .send_event(gst::event::StreamStart::new("aac-test"))
        );
        let caps = gst::Caps::builder("audio/mpeg")
            .field("mpegversion", if mpeg2 { 2_i32 } else { 4_i32 })
            .field("stream-format", "adts")
            .build();
        assert!(test.input.send_event(gst::event::Caps::new(&caps)));
        test.segment();
        Ok(test)
    }

    fn segment(&self) {
        assert!(
            self.input
                .send_event(gst::event::Segment::new(&gst::FormattedSegment::<
                    gst::ClockTime,
                >::new(),))
        );
    }

    fn push(&self, bytes: &[u8]) -> TestResult {
        let mut buffer = gst::Buffer::from_slice(bytes.to_vec());
        buffer.make_mut().set_pts(gst::ClockTime::ZERO);
        self.input.chain(buffer)?;
        Ok(())
    }

    fn finish(&self) {
        assert!(self.input.send_event(gst::event::Eos::new()));
    }

    fn bytes(&self) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        self.output
            .lock()
            .expect("test output mutex")
            .iter()
            .map(|buffer| Ok(buffer.map_readable()?.as_slice().to_vec()))
            .collect()
    }
}

impl Drop for Parser {
    fn drop(&mut self) {
        if let Err(error) = self.pipeline.set_state(gst::State::Null) {
            tracing::error!(%error, "Failed to stop AAC parser test");
        }
    }
}

#[test]
fn fragmented_larger_adts_frames_preserve_every_byte() -> TestResult {
    for mpeg2 in [false, true] {
        for crc in [false, true] {
            let small = adts(SMALL_FRAME_BYTES, mpeg2, crc);
            let large = adts(LARGE_FRAME_BYTES, mpeg2, crc);
            for split in 1..large.len() {
                let parser = Parser::new(mpeg2)?;
                parser.push(&[small.as_slice(), &small].concat())?;
                parser.push(&large[..split])?;
                parser.push(&[&large[split..], &small, &small].concat())?;
                // Do not keep the large frame's wait threshold after it is
                // complete: small live frames must be delivered before EOS.
                assert_eq!(parser.bytes()?.len(), 5, "small frames remained buffered");
                parser.finish();
                assert_eq!(
                    parser.bytes()?,
                    [&small, &small, &large, &small, &small].map(Clone::clone),
                    "MPEG-{} CRC={crc} split={split}",
                    if mpeg2 { 2 } else { 4 }
                );
            }
        }
    }
    Ok(())
}

#[test]
fn truncated_final_frame_is_drained_without_waiting_forever() -> TestResult {
    let parser = Parser::new(true)?;
    let small = adts(SMALL_FRAME_BYTES, true, false);
    let large = adts(LARGE_FRAME_BYTES, true, false);
    parser.push(&[small.as_slice(), &small].concat())?;
    parser.push(&large[..large.len() / 2])?;
    parser.finish();
    assert_eq!(parser.bytes()?, [&small, &small].map(Clone::clone));
    Ok(())
}

#[test]
fn flush_discards_only_the_unfinished_frame() -> TestResult {
    let parser = Parser::new(true)?;
    let small = adts(SMALL_FRAME_BYTES, true, false);
    let large = adts(LARGE_FRAME_BYTES, true, false);
    parser.push(&[small.as_slice(), &small].concat())?;
    parser.push(&large[..large.len() / 2])?;
    let before = parser.bytes()?;
    assert_eq!(before, [&small, &small].map(Clone::clone));
    assert!(parser.input.send_event(gst::event::FlushStart::new()));
    assert!(parser.input.send_event(gst::event::FlushStop::new(false)));
    parser.segment();
    parser.push(&[small.as_slice(), &small].concat())?;
    parser.finish();
    assert_eq!(
        parser.bytes()?,
        [&small, &small, &small, &small].map(Clone::clone)
    );
    Ok(())
}

#[test]
fn parser_keeps_frame_durations_and_recovers_after_junk() -> TestResult {
    let parser = Parser::new(true)?;
    let frame = adts(SMALL_FRAME_BYTES, true, false);
    parser.push(&[&[0; ADTS_HEADER_BYTES], frame.as_slice(), &frame, &frame].concat())?;
    parser.finish();
    assert_eq!(parser.bytes()?, [&frame, &frame, &frame].map(Clone::clone));
    for buffer in parser.output.lock().expect("test output mutex").iter() {
        assert_eq!(
            buffer.duration().ok_or("duration")?.nseconds(),
            SAMPLES_PER_FRAME * gst::ClockTime::SECOND.nseconds() / SAMPLE_RATE
        );
    }
    Ok(())
}

fn decode_recording(path: &std::path::Path) -> TestResult {
    use std::sync::atomic::{AtomicBool, Ordering};
    gst::init()?;
    register()?;
    let player = crate::playback::clock::Policy::Automatic.build_playbin()?;
    let input = crate::playback::input::Input::file(&player, path, 0, false)?;
    struct Playback(gst::Element, crate::playback::input::Input);
    impl Drop for Playback {
        fn drop(&mut self) {
            self.1.suspend(true);
            if let Err(error) = self.0.set_state(gst::State::Null) {
                tracing::error!(%error, "Failed to stop AAC decoding test");
            }
        }
    }
    let playback = Playback(player, input);
    let selected = Arc::new(AtomicBool::new(false));
    let configured = selected.clone();
    playback.0.connect("element-setup", false, move |values| {
        let element = values[1]
            .get::<gst::Element>()
            .expect("element-setup element");
        if element
            .factory()
            .is_some_and(|factory| factory.name() == FACTORY)
        {
            configured.store(true, Ordering::Relaxed);
        }
        None
    });
    let output = Arc::new(Mutex::new(Vec::new()));
    let captured = output.clone();
    let sink = gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .property("signal-handoffs", true)
        .build()?;
    sink.connect("handoff", false, move |values| {
        let buffer = values[1].get::<gst::Buffer>().expect("handoff buffer");
        let pad = values[2].get::<gst::Pad>().expect("handoff pad");
        let stream = pad
            .sticky_event::<gst::event::StreamStart>(0)
            .expect("audio stream-start")
            .seqnum();
        captured
            .lock()
            .expect("test output mutex")
            .push((buffer.pts(), buffer.duration(), stream));
        None
    });
    playback.0.set_property_from_str("flags", "audio");
    playback.0.set_property("audio-sink", &sink);
    let tempo = gst::ElementFactory::make("scaletempo").build()?;
    let filter =
        crate::playback::audio_routing::Routing::default().filter_with_tempo(Some(&tempo))?;
    playback.0.set_property("audio-filter", &filter);
    playback.0.set_property("uri", "appsrc://");
    playback.0.set_state(gst::State::Playing)?;
    let message = playback
        .0
        .bus()
        .ok_or("bus")?
        .timed_pop_filtered(
            gst::ClockTime::from_seconds(30),
            &[gst::MessageType::Error, gst::MessageType::Eos],
        )
        .ok_or("audio decode timed out")?;
    assert_eq!(message.type_(), gst::MessageType::Eos, "{message:?}");
    playback.1.check()?;
    assert!(
        selected.load(Ordering::Relaxed),
        "playbin did not select guarded AAC parser"
    );
    let frames = output.lock().expect("test output mutex");
    assert!(
        frames.len() > 100,
        "too few decoded frames: {}",
        frames.len()
    );
    let timestamps: Vec<_> = frames
        .iter()
        .map(|(pts, duration, stream)| {
            Ok((
                pts.ok_or("missing audio PTS")?,
                duration.ok_or("missing audio duration")?,
                *stream,
            ))
        })
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;
    // A PMT change may replace the entire parser while it has a partial frame.
    // Keep those stream-boundary gaps visible, but verify this guard's contract
    // within each uninterrupted stream; it cannot retain another parser's data.
    let gaps: Vec<_> = timestamps
        .windows(2)
        .filter_map(|pair| {
            let expected = pair[0].0 + pair[0].1;
            let actual = pair[1].0;
            (actual.absdiff(expected) > TIMESTAMP_ROUNDING).then_some((
                expected,
                actual,
                pair[0].2 == pair[1].2,
            ))
        })
        .collect();
    let continuous_gaps: Vec<_> = gaps.iter().filter(|gap| gap.2).collect();
    eprintln!(
        "AAC decoded frames={} continuity_gaps={} stream_boundary_gaps={}",
        frames.len(),
        continuous_gaps.len(),
        gaps.len() - continuous_gaps.len()
    );
    assert!(
        continuous_gaps.is_empty(),
        "decoded audio has timestamp gaps: {:?}",
        &continuous_gaps[..continuous_gaps.len().min(8)]
    );
    Ok(())
}

#[test]
fn playbin_selects_guard_and_decodes_continuous_audio() -> TestResult {
    decode_recording(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/recording-seek.ts"),
    )
}

#[test]
#[ignore = "CPU-only audio probe; requires NAGAMETV_RECORDING_PROBE pointing to a short TS capture"]
fn captured_broadcast_keeps_audio_frames() -> TestResult {
    let path = std::env::var_os("NAGAMETV_RECORDING_PROBE").ok_or("NAGAMETV_RECORDING_PROBE")?;
    decode_recording(std::path::Path::new(&path))
}
