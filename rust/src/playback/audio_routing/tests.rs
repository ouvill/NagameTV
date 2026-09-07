use super::*;
use std::{sync::mpsc, time::Duration};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn generations_and_stream_identity_reject_old_requests() -> TestResult {
    let routing = Routing::default();
    assert!(matches!(
        routing.select(routing.format(), Mode::Main),
        Err(Error::Unsupported)
    ));
    assert!(routing.stream_start("old"));
    routing.renegotiate(true);
    let old = routing.format();
    routing.select_for(old, "old", Mode::Main)?;
    assert_eq!(routing.format().mode, Some(Mode::Main));
    assert!(routing.stream_start("new"));
    assert_eq!(routing.format().mode, Some(Mode::Both));
    assert!(matches!(
        routing.select_for(old, "new", Mode::Sub),
        Err(Error::Changed)
    ));
    assert!(
        routing
            .select_for(routing.format(), "old", Mode::Sub)
            .is_err()
    );
    routing.renegotiate(false);
    assert!(matches!(
        routing.select_for(routing.format(), "new", Mode::Sub),
        Err(Error::Unsupported)
    ));
    routing.renegotiate(true);
    routing.select_for(routing.format(), "new", Mode::Sub)?;
    routing.reset();
    assert_eq!(routing.format().mode, None);
    Ok(())
}

#[test]
fn routing_preserves_sample_bits_and_rejects_partial_frames() -> TestResult {
    let original = [1, 0, 0xc0, 0x7f, 0, 0, 0, 0x80];
    for mode in [Mode::Main, Mode::Sub, Mode::Both] {
        let mut data = original;
        route(&mut data, mode)?;
        let expected = match mode {
            Mode::Main => [1, 0, 0xc0, 0x7f, 1, 0, 0xc0, 0x7f],
            Mode::Sub => [0, 0, 0, 0x80, 0, 0, 0, 0x80],
            Mode::Both => original,
        };
        assert_eq!(data, expected);
    }
    let mut partial = [0_u8; 9];
    assert!(route(&mut partial, Mode::Main).is_err());
    assert_eq!(partial, [0_u8; 9]);
    Ok(())
}

struct Pipeline(gst::Pipeline);
impl Drop for Pipeline {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}

#[test]
fn native_transform_routes_shared_buffers_without_changing_timing_or_input() -> TestResult {
    gst::init()?;
    let routing = Routing::default();
    let filter = routing.filter()?;
    let pipeline = Pipeline(gst::Pipeline::new());
    let caps = gst::Caps::builder("audio/x-raw")
        .field("format", "F32LE")
        .field("layout", "interleaved")
        .field("rate", 48000_i32)
        .field("channels", 2_i32)
        .build();
    let source = gst::ElementFactory::make("appsrc")
        .property("caps", &caps)
        .property("format", gst::Format::Time)
        .build()?;
    // Explicit CPU fixture. No device is selected or used.
    let sink = gst::ElementFactory::make("fakesink")
        .property("signal-handoffs", true)
        .build()?;
    let (tx, rx) = mpsc::sync_channel(1);
    sink.connect("handoff", false, move |values| {
        if let Some(buffer) = values
            .get(1)
            .and_then(|value| value.get::<gst::Buffer>().ok())
        {
            let _ = tx.try_send(buffer);
        }
        None
    });
    pipeline.0.add_many([&source, filter.upcast_ref(), &sink])?;
    gst::Element::link_many([&source, filter.upcast_ref(), &sink])?;
    pipeline.0.set_state(gst::State::Playing)?;
    let original = [0, 0, 0x80, 0x3f, 0, 0, 0, 0x40];
    let mut input = gst::Buffer::from_mut_slice(original.to_vec());
    let buffer = input.get_mut().ok_or("new input unexpectedly shared")?;
    buffer.set_pts(gst::ClockTime::from_mseconds(10));
    buffer.set_duration(gst::ClockTime::from_nseconds(20833));
    let push = || -> Result<gst::Buffer, Box<dyn std::error::Error>> {
        source
            .emit_by_name::<gst::FlowReturn>("push-buffer", &[&input])
            .into_result()?;
        Ok(rx.recv_timeout(Duration::from_secs(5))?)
    };
    assert_eq!(push()?.map_readable()?.as_slice(), original);
    let stream = routing
        .0
        .stream
        .lock()
        .map_err(|_| "poisoned test state")?
        .clone()
        .ok_or("stream-start missing")?;
    for mode in [Mode::Main, Mode::Sub, Mode::Both] {
        routing.select_for(routing.format(), &stream, mode)?;
        let output = push()?;
        let mut expected = original;
        route(&mut expected, mode)?;
        assert_eq!(output.map_readable()?.as_slice(), expected);
        assert_eq!(output.pts(), input.pts());
        assert_eq!(output.duration(), input.duration());
        assert_eq!(input.map_readable()?.as_slice(), original);
    }
    routing.select_for(routing.format(), &stream, Mode::Sub)?;
    let before_flush = routing.format();
    let pad = source
        .static_pad("src")
        .ok_or("appsrc source pad missing")?;
    // Flush through appsrc so its queue and source task participate as well.
    // Pushing directly from the pad can leave the source task stopped on FLUSHING.
    assert!(source.send_event(gst::event::FlushStart::new()));
    assert!(source.send_event(gst::event::FlushStop::new(false)));
    let segment = gst::FormattedSegment::<gst::ClockTime>::new();
    assert!(pad.push_event(gst::event::Segment::new(&segment)));
    // No new CAPS or STREAM_START: flushing a stream is not a format change.
    assert_eq!(routing.format().mode, Some(Mode::Both));
    assert!(matches!(
        routing.select_for(before_flush, &stream, Mode::Sub),
        Err(Error::Changed)
    ));
    assert_eq!(push()?.map_readable()?.as_slice(), original);
    routing.select_for(routing.format(), &stream, Mode::Main)?;
    let mut expected = original;
    route(&mut expected, Mode::Main)?;
    assert_eq!(push()?.map_readable()?.as_slice(), expected);
    pipeline.0.set_state(gst::State::Null)?;
    assert_eq!(routing.format().mode, None);
    Ok(())
}
