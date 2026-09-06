//! CPU/network-only tests: the explicit fakesink consumes bytes, never video/audio hardware.
use super::*;
use crate::audio::{AudioProgram, ProgramAudio};
use std::io::{Read, Write};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// Always shut down the test pipeline, including on an early assertion/error.
struct RunningPipeline(gst::Pipeline);
impl Drop for RunningPipeline {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}

#[test]
fn ready_closes_old_http_body_and_releases_queued_messages() -> TestResult {
    gst::init()?;
    let pipeline = RunningPipeline(gst::Pipeline::new());
    let source = gst::ElementFactory::make("souphttpsrc")
        .property("retries", 0_i32)
        .build()?;
    let queue = gst::ElementFactory::make("queue").build()?;
    let sink = gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .property("enable-last-sample", false)
        .property("signal-handoffs", true)
        .build()?;
    pipeline.0.add_many([&source, &queue, &sink])?;
    gst::Element::link_many([&source, &queue, &sink])?;
    let bus = pipeline.0.bus().ok_or("missing bus")?;
    let (received_tx, received_rx) = std::sync::mpsc::sync_channel(1);
    sink.connect("handoff", false, move |_| {
        let _ = received_tx.try_send(());
        None
    });
    // Reuse the same pipeline repeatedly, as channel switching does.
    for _ in 0..5 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        source.set_property(
            "location",
            format!("http://{}/stream", listener.local_addr()?),
        );
        let (closed_tx, closed_rx) = std::sync::mpsc::sync_channel(1);
        let server = std::thread::spawn(move || -> std::io::Result<()> {
            // Bounded accept and socket reads keep test failures from leaving workers alive.
            listener.set_nonblocking(true)?;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => return Err(error),
                }
            };
            socket.set_read_timeout(Some(Duration::from_secs(5)))?;
            socket.set_write_timeout(Some(Duration::from_secs(5)))?;
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte)?;
                request.push(byte[0]);
                if request.len() > 8192 {
                    return Err(std::io::Error::other("oversized request"));
                }
            }
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 1000000000\r\n\r\n")?;
            socket.write_all(&[0x47; 8192])?;
            // The response never ends on its own: READY must cancel its active request.
            let closed = match socket.read(&mut byte) {
                Ok(0) => true,
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => true,
                _ => false,
            };
            let _ = closed_tx.send(closed);
            Ok(())
        });
        pipeline.0.set_state(gst::State::Playing)?;
        received_rx.recv_timeout(Duration::from_secs(5))?;
        let stale = gst::ElementFactory::make("identity").build()?;
        let weak = stale.downgrade();
        bus.post(gst::message::Eos::builder().src(&stale).build())?;
        drop(stale);
        session::reset_pipeline(pipeline.0.upcast_ref(), &bus)?;
        assert!(
            closed_rx.recv_timeout(Duration::from_secs(5))?,
            "old HTTP body remained open"
        );
        server.join().map_err(|_| "HTTP worker panicked")??;
        assert_eq!(pipeline.0.current_state(), gst::State::Ready);
        assert_eq!(queue.property::<u32>("current-level-buffers"), 0);
        assert!(
            weak.upgrade().is_none(),
            "bus retained the old message source"
        );
        assert!(bus.pop().is_none());
        while received_rx.try_recv().is_ok() {}
    }
    Ok(())
}

#[test]
fn prepares_metadata_and_releases_old_stream_state_before_start() -> TestResult {
    gst::init()?;
    // No Qt item is attached and no output device is created or started.
    let playbin = gst::Pipeline::new().upcast::<gst::Element>();
    let identity = gst::ElementFactory::make("identity").build()?;
    let playback = Playback {
        playbin,
        video_sink: identity.clone(),
        video_process: identity.clone(),
        video_queue: identity,
        deinterlace_mode: DeinterlaceMode::Off,
        video_attached: false,
        subtitles: SubtitleClock::default(),
        audio: RefCell::new(AudioStreams::default()),
        extractor: Arc::new(Mutex::new(TsSubtitleExtractor::new())),
        routing: AudioRouting::default(),
    };
    let bus = playback.playbin.bus().ok_or("missing bus")?;
    let old_stream = gst::Stream::new(
        Some("old/00000201"),
        None,
        gst::StreamType::AUDIO,
        gst::StreamFlags::empty(),
    );
    let weak = old_stream.downgrade();
    {
        let collection = gst::StreamCollection::builder(None)
            .stream(old_stream.clone())
            .build();
        playback.audio.borrow_mut().observe(
            &playback.playbin,
            &gst::message::StreamCollection::new(&collection),
        );
    }
    drop(old_stream);
    playback
        .subtitles
        .push(vec![crate::subtitles::SubtitleCue::clear(100)]);
    playback
        .extractor
        .lock()
        .map_err(|_| "extractor poisoned")?
        .push(include_bytes!("../../../tests/fixtures/subtitle-clock.ts"));
    let program = AudioProgram {
        service_id: 20,
        start_at: 1000,
        audios: vec![ProgramAudio {
            component_tag: 16,
            component_type: 2,
            is_main: true,
            langs: vec!["jpn".into(), "eng".into()],
        }],
    };
    playback.prepare_stream(&bus, Some(program.clone()))?;
    assert_eq!(playback.playbin.current_state(), gst::State::Ready);
    assert_eq!(playback.audio.borrow().program, Some(program));
    assert!(weak.upgrade().is_none(), "old audio stream was retained");
    assert_eq!(playback.pending_subtitles(), Some(0));
    assert!(
        playback
            .extractor
            .lock()
            .map_err(|_| "extractor poisoned")?
            .audio_components
            .is_empty()
    );
    playback.prepare_stream(&bus, None)?;
    assert!(playback.audio.borrow().program.is_none());
    playback.stop()?;
    assert_eq!(playback.playbin.current_state(), gst::State::Null);
    Ok(())
}
