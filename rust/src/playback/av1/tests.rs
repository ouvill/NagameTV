//! CPU-only parser tests: encoded buffers and memory sinks, with no decoder,
//! display, GPU, audio device or Qt application.
use super::*;
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn caps(alignment: &str) -> gst::Caps {
    gst::Caps::builder("video/x-av1")
        .field("stream-format", "obu-stream")
        .field("alignment", alignment)
        .build()
}

fn state() -> Restoration {
    gst::init().expect("GStreamer");
    let mut state = Restoration::default();
    state.sink_event(&gst::event::Caps::new(&caps("tu")));
    state.src_event(&gst::event::Caps::new(&caps("frame")));
    state
}

fn buffer(pts: Option<u64>, duration: Option<u64>, flags: gst::BufferFlags) -> gst::Buffer {
    let mut buffer = gst::Buffer::from_slice(vec![0_u8; 4]);
    let writable = buffer.make_mut();
    writable.set_pts(pts.map(gst::ClockTime::from_mseconds));
    writable.set_duration(duration.map(gst::ClockTime::from_mseconds));
    writable.set_flags(flags);
    buffer
}

fn unit(pts: u64, duration: u64) -> gst::Buffer {
    buffer(Some(pts), Some(duration), gst::BufferFlags::empty())
}

fn missing_marker() -> gst::Buffer {
    buffer(None, None, gst::BufferFlags::MARKER)
}

fn assert_timing(buffer: &gst::BufferRef, pts: Option<u64>, duration: Option<u64>) {
    assert_eq!(buffer.pts(), pts.map(gst::ClockTime::from_mseconds));
    assert_eq!(
        buffer.duration(),
        duration.map(gst::ClockTime::from_mseconds)
    );
}

fn with_bytes(mut buffer: gst::Buffer, bytes: &[u8]) -> gst::Buffer {
    buffer.make_mut().remove_all_memory();
    buffer
        .make_mut()
        .append_memory(gst::Memory::from_slice(bytes.to_vec()));
    buffer
}

#[test]
fn hidden_frames_share_vfr_timing_regardless_of_marker_placement() {
    for marker in [gst::BufferFlags::empty(), gst::BufferFlags::MARKER] {
        let mut state = state();
        state.sink_buffer(&with_bytes(unit(100, 33), &[1, 2, 3, 4]));
        state.sink_buffer(&with_bytes(unit(133, 67), &[5]));
        for byte in 1..=4 {
            let mut output = with_bytes(
                buffer(
                    None,
                    None,
                    marker
                        | if byte < 4 {
                            gst::BufferFlags::DECODE_ONLY | gst::BufferFlags::DELTA_UNIT
                        } else {
                            gst::BufferFlags::empty()
                        },
                ),
                &[byte],
            );
            let flags = output.flags();
            state.src_buffer(output.make_mut());
            assert_timing(&output, Some(100), Some(33));
            assert_eq!(output.flags(), flags);
            assert_eq!(output.map_readable().unwrap().as_slice(), &[byte]);
        }
        let mut next = with_bytes(missing_marker(), &[5]);
        state.src_buffer(next.make_mut());
        assert_timing(&next, Some(133), Some(67));
        assert!(!state.warned);
    }
}

#[test]
fn dropped_units_and_partial_output_do_not_shift_later_timestamps() {
    let mut state = state();
    state.sink_buffer(&with_bytes(unit(100, 33), &[1, 2]));
    state.sink_buffer(&with_bytes(unit(133, 67), &[3]));
    state.sink_buffer(&with_bytes(unit(200, 33), &[4]));
    let mut hidden = with_bytes(missing_marker(), &[1]);
    state.src_buffer(hidden.make_mut());
    assert_timing(&hidden, Some(100), Some(33));
    // The parser drops the rest of TU 0 and all of TU 1.
    let mut later = with_bytes(missing_marker(), &[4]);
    state.src_buffer(later.make_mut());
    assert_timing(&later, Some(200), Some(33));
    assert!(!state.warned);
}

proptest! {
    #[test]
    fn arbitrary_drops_and_markers_keep_each_frames_own_tu_timing(
        units in proptest::collection::vec(
            (1_u64..100, proptest::collection::vec((any::<bool>(), any::<bool>()), 1..6)),
            1..32,
        )
    ) {
        let mut state = state();
        let mut outputs = Vec::new();
        let mut pts = 100;
        for (index, (duration, frames)) in units.iter().enumerate() {
            let mut payload = Vec::new();
            for (frame, &(emitted, marker)) in frames.iter().enumerate() {
                let bytes = [0xff, index as u8, frame as u8, 0xfe];
                payload.extend_from_slice(&bytes);
                if emitted {
                    outputs.push((bytes, pts, *duration, marker));
                }
            }
            state.sink_buffer(&with_bytes(unit(pts, *duration), &payload));
            pts += duration;
        }
        for (bytes, pts, duration, marker) in outputs {
            let flags = if marker { gst::BufferFlags::MARKER } else { gst::BufferFlags::empty() };
            let mut output = with_bytes(buffer(None, None, flags), &bytes);
            state.src_buffer(output.make_mut());
            prop_assert_eq!(output.pts(), Some(gst::ClockTime::from_mseconds(pts)));
            prop_assert_eq!(output.duration(), Some(gst::ClockTime::from_mseconds(duration)));
        }
        prop_assert!(!state.warned);
    }
}

#[test]
fn ambiguous_or_changed_payloads_suspend_without_guessing() {
    for bytes in [&[0_u8; 4][..], &[1_u8][..], &[][..]] {
        let mut state = state();
        state.sink_buffer(&unit(100, 33));
        state.sink_buffer(&unit(133, 67));
        let mut output = with_bytes(missing_marker(), bytes);
        state.src_buffer(output.make_mut());
        assert_timing(&output, None, None);
        assert!(state.warned);
        state.sink_buffer(&with_bytes(unit(200, 33), &[2]));
        let mut later = with_bytes(missing_marker(), &[2]);
        state.src_buffer(later.make_mut());
        assert_timing(&later, None, None);
    }
}

#[test]
fn repeated_bytes_within_one_unit_keep_that_units_timing() {
    let mut state = state();
    state.sink_buffer(&with_bytes(unit(100, 33), &[1, 1]));
    for _ in 0..2 {
        let mut output = with_bytes(missing_marker(), &[1]);
        state.src_buffer(output.make_mut());
        assert_timing(&output, Some(100), Some(33));
    }
    assert!(!state.warned);
}

#[test]
fn payload_memory_is_bounded() {
    let mut state = state();
    state.sink_buffer(&with_bytes(unit(100, 33), &vec![0; MAX_PENDING_BYTES]));
    assert!(!state.warned);
    state.sink_buffer(&unit(133, 67));
    assert!(state.warned);
    let mut output = missing_marker();
    state.src_buffer(output.make_mut());
    assert_timing(&output, None, None);
}

#[test]
fn valid_pts_duration_and_dts_are_independently_preserved() {
    let mut state = state();
    for (pts, duration) in [(Some(120), None), (None, Some(44)), (Some(120), Some(44))] {
        state.sink_buffer(&unit(100, 33));
        let mut output = buffer(pts, duration, gst::BufferFlags::MARKER);
        output.make_mut().set_dts(gst::ClockTime::from_mseconds(90));
        state.src_buffer(output.make_mut());
        assert_timing(&output, pts.or(Some(100)), duration.or(Some(33)));
        assert_eq!(output.dts(), Some(gst::ClockTime::from_mseconds(90)));
    }
}

#[test]
fn absent_input_timestamps_are_not_invented() {
    let mut state = state();
    state.sink_buffer(&buffer(None, None, gst::BufferFlags::empty()));
    let mut output = missing_marker();
    state.src_buffer(output.make_mut());
    assert_timing(&output, None, None);
    assert!(!state.warned);
}

#[test]
fn only_temporal_unit_to_frame_conversion_is_modified() {
    for (input, output) in [
        ("tu", "tu"),
        ("frame", "frame"),
        ("obu", "frame"),
        ("tu", "obu"),
    ] {
        let mut state = state();
        state.sink_event(&gst::event::Caps::new(&caps(input)));
        state.src_event(&gst::event::Caps::new(&caps(output)));
        for _ in 0..MAX_PENDING_UNITS + 1 {
            state.sink_buffer(&unit(100, 33));
            let mut output = missing_marker();
            state.src_buffer(output.make_mut());
            assert_timing(&output, None, None);
        }
        assert!(!state.warned);
    }
}

#[test]
fn format_conversion_or_unspecified_format_is_left_unchanged() {
    for input_format in [Some("obu-stream"), Some("annexb"), None] {
        for output_format in [Some("obu-stream"), Some("annexb"), None] {
            if input_format == Some("obu-stream") && output_format == input_format {
                continue;
            }
            let mut state = state();
            let make_caps = |alignment, format: Option<&str>| {
                let builder = gst::Caps::builder("video/x-av1").field("alignment", alignment);
                match format {
                    Some(format) => builder.field("stream-format", format).build(),
                    None => builder.build(),
                }
            };
            state.sink_event(&gst::event::Caps::new(&make_caps("tu", input_format)));
            state.src_event(&gst::event::Caps::new(&make_caps("frame", output_format)));
            state.sink_buffer(&unit(100, 33));
            let mut output = missing_marker();
            state.src_buffer(output.make_mut());
            assert_timing(&output, None, None);
            assert!(!state.warned);
        }
    }
}

#[test]
fn flush_segment_caps_and_discontinuity_discard_previous_timing() {
    gst::init().unwrap();
    for event in [
        gst::event::FlushStop::new(false),
        gst::event::Segment::new(&gst::FormattedSegment::<gst::ClockTime>::new()),
        gst::event::Caps::new(&caps("tu")),
    ] {
        let mut state = state();
        state.sink_buffer(&unit(100, 33));
        state.sink_event(&event);
        state.sink_buffer(&unit(900, 67));
        let mut output = missing_marker();
        state.src_buffer(output.make_mut());
        assert_timing(&output, Some(900), Some(67));
    }
    let mut state = state();
    state.sink_buffer(&unit(100, 33));
    state.sink_buffer(&buffer(Some(900), Some(67), gst::BufferFlags::DISCONT));
    let mut output = missing_marker();
    state.src_buffer(output.make_mut());
    assert_timing(&output, Some(900), Some(67));
}

#[test]
fn output_during_flush_cannot_consume_stale_timestamps_or_report_desync() {
    let mut state = state();
    state.sink_buffer(&unit(100, 33));
    state.sink_event(&gst::event::FlushStart::new());
    let mut output = missing_marker();
    state.src_buffer(output.make_mut());
    assert_timing(&output, None, None);
    assert!(!state.warned);
    state.sink_event(&gst::event::FlushStop::new(false));
    state.sink_buffer(&unit(900, 67));
    state.src_buffer(output.make_mut());
    assert_timing(&output, Some(900), Some(67));
}

#[test]
fn buffer_lists_preserve_order_and_suspend_on_internal_discontinuity() {
    let mut state = state();
    let mut list = gst::BufferList::new();
    list.make_mut().add(with_bytes(unit(100, 33), &[1]));
    list.make_mut().add(with_bytes(unit(133, 67), &[2]));
    state.sink_list(&list);
    for (byte, pts, duration) in [(1, 100, 33), (2, 133, 67)] {
        let mut output = with_bytes(missing_marker(), &[byte]);
        state.src_buffer(output.make_mut());
        assert_timing(&output, Some(pts), Some(duration));
    }
    list.make_mut()
        .get_mut(1)
        .unwrap()
        .set_flags(gst::BufferFlags::DISCONT);
    state.sink_list(&list);
    let mut output = missing_marker();
    state.src_buffer(output.make_mut());
    assert_timing(&output, None, None);
    assert!(state.warned);
}

#[test]
fn new_stream_requires_new_caps_and_keeps_first_buffer_through_deferred_events() {
    let mut state = state();
    state.sink_buffer(&unit(100, 33));
    state.sink_event(&gst::event::StreamStart::new("next-stream"));
    state.sink_buffer(&unit(200, 33));
    let mut output = missing_marker();
    state.src_buffer(output.make_mut());
    assert_timing(&output, None, None);

    state.sink_event(&gst::event::Caps::new(&caps("tu")));
    state.sink_buffer(&unit(900, 67));
    state.src_event(&gst::event::Caps::new(&caps("frame")));
    state.src_event(&gst::event::Segment::new(&gst::FormattedSegment::<
        gst::ClockTime,
    >::new()));
    state.src_buffer(output.make_mut());
    assert_timing(&output, Some(900), Some(67));
}

#[test]
fn eos_allows_drain_then_discards_unconsumed_timestamps() {
    let mut state = state();
    state.sink_buffer(&unit(100, 33));
    state.sink_buffer(&with_bytes(unit(133, 33), &[1]));
    state.sink_event(&gst::event::Eos::new());
    let mut output = missing_marker();
    state.src_buffer(output.make_mut());
    assert_timing(&output, Some(100), Some(33));
    state.src_event(&gst::event::Eos::new());
    let mut after_eos = missing_marker();
    state.src_buffer(after_eos.make_mut());
    assert_timing(&after_eos, None, None);

    state.sink_event(&gst::event::FlushStop::new(false));
    state.sink_buffer(&unit(900, 67));
    state.src_buffer(after_eos.make_mut());
    assert_timing(&after_eos, Some(900), Some(67));
}

#[test]
fn unmatched_output_suspends_restoration_until_reset() {
    let mut state = state();
    let mut output = missing_marker();
    state.src_buffer(output.make_mut());
    assert_timing(&output, None, None);
    assert!(state.warned);
    state.sink_buffer(&unit(100, 33));
    state.src_buffer(output.make_mut());
    assert_timing(&output, None, None);

    state.sink_event(&gst::event::FlushStop::new(false));
    state.sink_buffer(&unit(900, 67));
    state.src_buffer(output.make_mut());
    assert_timing(&output, Some(900), Some(67));
    assert!(state.warned, "warning suppression survives a stream reset");
}

#[test]
fn excessive_pending_units_suspend_without_reusing_old_timestamps() {
    let mut state = state();
    for index in 0..MAX_PENDING_UNITS {
        state.sink_buffer(&unit(index as u64, 33));
    }
    assert!(!state.warned);
    state.sink_buffer(&unit(800, 33));
    assert!(state.warned);
    for _ in 0..MAX_PENDING_UNITS * 2 {
        state.sink_buffer(&unit(900, 33));
    }
    let mut output = missing_marker();
    state.src_buffer(output.make_mut());
    assert_timing(&output, None, None);
    state.sink_event(&gst::event::FlushStop::new(false));
    state.sink_buffer(&unit(1_000, 67));
    state.src_buffer(output.make_mut());
    assert_timing(&output, Some(1_000), Some(67));
}

#[derive(Default)]
struct Observations {
    input: Vec<gst::Buffer>,
    original: Vec<gst::Buffer>,
    restored: Vec<gst::Buffer>,
}

struct Pipeline(gst::Pipeline);

impl Drop for Pipeline {
    fn drop(&mut self) {
        if let Err(error) = self.0.set_state(gst::State::Null) {
            tracing::error!(%error, "Failed to stop AV1 parser test");
        }
    }
}

fn parse_fixture(alignment: &str) -> Result<Observations, Box<dyn std::error::Error>> {
    gst::init()?;
    let pipeline = Pipeline(gst::parse::launch(
        "filesrc name=input ! qtdemux ! av1parse name=parser ! capsfilter name=alignment ! fakesink sync=false",
    )?.downcast::<gst::Pipeline>().map_err(|_| "test pipeline")?);
    pipeline.0.by_name("input").ok_or("input")?.set_property(
        "location",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/fixtures/av1-timestamps.mp4"
        ),
    );
    pipeline
        .0
        .by_name("alignment")
        .ok_or("capsfilter")?
        .set_property("caps", caps(alignment));
    let parser = pipeline.0.by_name("parser").ok_or("parser")?;
    let observations = Arc::new(Mutex::new(Observations::default()));
    let input = observations.clone();
    parser.static_pad("sink").ok_or("sink pad")?.add_probe(
        gst::PadProbeType::BUFFER,
        move |_, info| {
            let mut observations = input.lock().unwrap();
            let buffer = info.buffer_mut().expect("input buffer").make_mut();
            // Exercise VFR timing independently of the encoder's fixed input rate.
            let index = observations.input.len() as u64;
            buffer.set_pts(gst::ClockTime::from_mseconds(
                index / 2 * 100 + index % 2 * 33,
            ));
            buffer.set_duration(gst::ClockTime::from_mseconds(if index.is_multiple_of(2) {
                33
            } else {
                67
            }));
            observations.input.push(buffer.to_owned());
            gst::PadProbeReturn::Ok
        },
    );
    let original = observations.clone();
    let src = parser.static_pad("src").ok_or("src pad")?;
    src.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        original
            .lock()
            .unwrap()
            .original
            .push(info.buffer().expect("original buffer").clone());
        gst::PadProbeReturn::Ok
    });
    configure(&parser)?;
    let restored = observations.clone();
    src.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        restored
            .lock()
            .unwrap()
            .restored
            .push(info.buffer().expect("restored buffer").clone());
        gst::PadProbeReturn::Ok
    });
    pipeline.0.set_state(gst::State::Playing)?;
    let message = pipeline
        .0
        .bus()
        .ok_or("bus")?
        .timed_pop_filtered(
            gst::ClockTime::from_seconds(5),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        )
        .ok_or("AV1 parser test timed out")?;
    match message.view() {
        gst::MessageView::Error(error) => {
            return Err(format!("{} ({:?})", error.error(), error.debug()).into());
        }
        gst::MessageView::Eos(_) => {}
        _ => unreachable!("filtered message"),
    }
    pipeline.0.set_state(gst::State::Null)?;
    let mut observations = observations.lock().unwrap();
    Ok(std::mem::take(&mut *observations))
}

#[test]
fn real_parser_frame_output_keeps_each_temporal_units_vfr_timestamps() -> TestResult {
    let observations = parse_fixture("frame")?;
    assert_eq!(observations.input.len(), 32);
    assert_eq!(observations.original.len(), observations.restored.len());
    assert!(
        observations
            .original
            .iter()
            .any(|b| b.flags().contains(gst::BufferFlags::DECODE_ONLY)),
        "fixture must contain hidden frames"
    );
    for (before, after) in observations.original.iter().zip(&observations.restored) {
        // Identify fixture provenance independently of parser flags, output
        // counts and restoration queue state, including inputs dropped by 1.24.
        let payload = before.map_readable()?;
        assert!(!payload.is_empty());
        let mut origins = Vec::new();
        for (index, input) in observations.input.iter().enumerate() {
            if input
                .map_readable()?
                .windows(payload.len())
                .any(|bytes| bytes == payload.as_slice())
            {
                origins.push(index);
            }
        }
        assert_eq!(
            origins.len(),
            1,
            "fixture frame must identify exactly one TU"
        );
        let unit = origins[0];
        let input = &observations.input[unit];
        assert_eq!(after.pts(), before.pts().or(input.pts()), "TU {unit}");
        assert_eq!(
            after.duration(),
            before.duration().or(input.duration()),
            "TU {unit}"
        );
        assert!(after.pts().is_some());
        assert!(after.duration().is_some());
        assert_eq!(after.dts(), before.dts());
        assert_eq!(after.flags(), before.flags());
        assert_eq!(
            after.map_readable()?.as_slice(),
            before.map_readable()?.as_slice()
        );
    }
    Ok(())
}

#[test]
fn real_parser_temporal_unit_output_is_unchanged() -> TestResult {
    let observations = parse_fixture("tu")?;
    assert_eq!(observations.input.len(), 32);
    assert!(!observations.original.is_empty());
    assert_eq!(observations.restored.len(), observations.original.len());
    for (before, after) in observations.original.iter().zip(&observations.restored) {
        assert_eq!(after.pts(), before.pts());
        assert_eq!(after.duration(), before.duration());
        assert_eq!(after.dts(), before.dts());
        assert_eq!(after.flags(), before.flags());
        assert_eq!(
            after.map_readable()?.as_slice(),
            before.map_readable()?.as_slice()
        );
    }
    Ok(())
}
