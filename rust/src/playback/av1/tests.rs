//! CPU-only parser tests: encoded buffers and memory sinks, with no decoder,
//! display, GPU, audio device or Qt application.
use super::*;

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

#[test]
fn hidden_frames_share_vfr_timing_until_the_temporal_unit_marker() {
    let mut state = state();
    state.sink_buffer(&unit(100, 33));
    state.sink_buffer(&unit(133, 67));
    for _ in 0..3 {
        let mut hidden = buffer(
            None,
            None,
            gst::BufferFlags::DECODE_ONLY | gst::BufferFlags::DELTA_UNIT,
        );
        let flags = hidden.flags();
        let bytes = hidden.map_readable().unwrap().as_slice().to_vec();
        state.src_buffer(hidden.make_mut());
        assert_timing(&hidden, Some(100), Some(33));
        assert_eq!(hidden.flags(), flags);
        assert_eq!(hidden.map_readable().unwrap().as_slice(), bytes);
    }
    let mut shown = missing_marker();
    state.src_buffer(shown.make_mut());
    assert_timing(&shown, Some(100), Some(33));
    let mut next = missing_marker();
    state.src_buffer(next.make_mut());
    assert_timing(&next, Some(133), Some(67));
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
    list.make_mut().add(unit(100, 33));
    list.make_mut().add(unit(133, 67));
    state.sink_list(&list);
    for (pts, duration) in [(100, 33), (133, 67)] {
        let mut output = missing_marker();
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
    state.sink_buffer(&unit(133, 33));
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
fn unmatched_marker_suspends_restoration_until_reset() {
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
    let mut unit = 0;
    for (before, after) in observations.original.iter().zip(&observations.restored) {
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
        if after.flags().contains(gst::BufferFlags::MARKER) {
            unit += 1;
        }
    }
    assert_eq!(unit, observations.input.len());
    Ok(())
}

#[test]
fn real_parser_temporal_unit_output_is_unchanged() -> TestResult {
    let observations = parse_fixture("tu")?;
    assert_eq!(observations.input.len(), 32);
    assert_eq!(observations.original.len(), 32);
    assert_eq!(observations.restored.len(), 32);
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
