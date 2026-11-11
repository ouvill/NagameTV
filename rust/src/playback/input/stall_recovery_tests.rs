//! Real HTTP reception/appsrc/playbin3; explicit CPU decoders and memory sinks.
use super::*;
use crate::playback::{
    live_recovery::{Action, Observation, Recovery, Status},
    stats,
};
use gstreamer::{self as gst, prelude::*};
use std::{io::Write, net::TcpListener, thread};

const SEND_BYTES: usize = TS_PACKET_SIZE * 8;
const SEND_INTERVAL: Duration = Duration::from_millis(40);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const REOPEN_POLL_INTERVAL: Duration = Duration::from_millis(1);
const DEADLINE: Duration = Duration::from_secs(20);

#[derive(Debug)]
enum Phase {
    Transition,
    Stalled,
    Reopened {
        wall_time: Instant,
        policy_time: Instant,
    },
}

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
struct Server {
    url: String,
    stopped: Arc<AtomicBool>,
    requests: Arc<std::sync::atomic::AtomicU64>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}/api/services/1/stream", listener.local_addr()?);
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let requests = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let received = requests.clone();
        let worker = thread::spawn(move || {
            let mut clients = Vec::new();
            while !stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        received.fetch_add(1, Ordering::AcqRel);
                        let stop = stop.clone();
                        clients.push(thread::spawn(move || -> std::io::Result<()> {
                            socket.set_read_timeout(Some(Duration::from_secs(2)))?;
                            socket.set_write_timeout(Some(Duration::from_secs(2)))?;
                            let mut request = Vec::new();
                            while !request.ends_with(b"\r\n\r\n") {
                                assert!(request.len() < 8192);
                                let mut byte = [0];
                                socket.read_exact(&mut byte)?;
                                request.push(byte[0]);
                            }
                            assert!(request.starts_with(b"GET /api/services/1/stream "));
                            let bytes = include_bytes!("../../../../tests/fixtures/live-transition-audio.ts");
                            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: video/mp2t\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len())?;
                            for block in bytes.chunks(SEND_BYTES) {
                                if stop.load(Ordering::Acquire) { break; }
                                socket.write_all(block)?;
                                thread::sleep(SEND_INTERVAL);
                            }
                            Ok(())
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("live recovery fixture accept: {error}"),
                }
            }
            for client in clients {
                match client.join().expect("live fixture worker panicked") {
                    Ok(()) => {}
                    // Stopping/replacing the real input closes its previous response.
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                        ) => {}
                    Err(error) => panic!("live fixture response: {error}"),
                }
            }
        });
        Ok(Self {
            url,
            stopped,
            requests,
            worker: Some(worker),
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().expect("live fixture server panicked");
        }
    }
}
struct Pipeline {
    player: gst::Element,
    input: Input,
    video: gst::Element,
    audio: gst::Element,
    blocked: Arc<AtomicBool>,
}
impl Pipeline {
    fn start(uri: &str) -> TestResult<Self> {
        let video = gst::ElementFactory::make("fakesink")
            .property("sync", true)
            .build()?;
        let audio = gst::ElementFactory::make("fakesink")
            .property("sync", true)
            .build()?;
        let blocked = Arc::new(AtomicBool::new(false));
        let drop_video = blocked.clone();
        video.static_pad("sink").ok_or("video pad")?.add_probe(
            gst::PadProbeType::BUFFER,
            move |_, _| {
                if drop_video.load(Ordering::Acquire) {
                    gst::PadProbeReturn::Drop
                } else {
                    gst::PadProbeReturn::Ok
                }
            },
        );
        let player = gst::ElementFactory::make("playbin3")
            .property("video-sink", &video)
            .property("audio-sink", &audio)
            .build()?;
        player.set_property_from_str("flags", "video+audio+soft-volume+force-sw-decoders");
        let input = Input::live(&player, uri, 1, Retention::Off.into(), false)?;
        let pipeline = Self {
            player,
            input,
            video,
            audio,
            blocked,
        };
        pipeline.player.set_property("uri", "appsrc://");
        pipeline.player.set_state(gst::State::Playing)?;
        Ok(pipeline)
    }
    fn observation(&self) -> TestResult<Observation> {
        if let Some(message) = self
            .player
            .bus()
            .ok_or("bus")?
            .pop_filtered(&[gst::MessageType::Error])
            && let gst::MessageView::Error(error) = message.view()
        {
            return Err(format!("{} {:?}", error.error(), error.debug()).into());
        }
        self.input.check()?;
        let Some(mut sample) = self.input.recovery_sample()? else {
            return Err("not live".into());
        };
        sample.video_output = stats::frame_counters(&self.video).map(|value| value.rendered);
        sample.audio_output = stats::frame_counters(&self.audio).map(|value| value.rendered);
        Ok(Observation::Live(sample))
    }
}
impl Drop for Pipeline {
    fn drop(&mut self) {
        self.input.suspend(true);
        if let Err(error) = self.player.set_state(gst::State::Null) {
            tracing::error!(%error, "Could not stop live recovery test pipeline");
        }
    }
}

#[test]
fn transition_stall_reopens_the_same_http_channel_and_confirms_new_decoded_output() -> TestResult {
    gst::init()?;
    let server = Server::new()?;
    let mut pipeline = Some(Pipeline::start(&server.url)?);
    let mut recovery = Recovery::default();
    let started = Instant::now();
    let mut policy_time = started;
    let mut phase = Phase::Transition;
    let restored_sample = loop {
        if started.elapsed() >= DEADLINE {
            return Err(format!(
                "live recovery test deadline reached: phase={phase:?}, status={:?}",
                recovery.status()
            )
            .into());
        }
        // Several frames between observations must not be a scheduling prerequisite.
        thread::sleep(if matches!(phase, Phase::Reopened { .. }) {
            REOPEN_POLL_INTERVAL
        } else {
            POLL_INTERVAL
        });
        let current = pipeline.as_ref().ok_or("pipeline")?;
        let observation = current.observation()?;
        let Observation::Live(sample) = observation else {
            unreachable!()
        };
        if matches!(phase, Phase::Transition)
            && current.input.recovery_diagnostic()?.contains("version=1")
            && sample.video_output.is_some_and(|count| count > 5)
        {
            current.blocked.store(true, Ordering::Release);
            phase = Phase::Stalled;
        }
        if matches!(phase, Phase::Transition) {
            continue;
        }
        // Inject only policy time; HTTP reception, normalizer, decoders and sink
        // counters are real. Unit tests verify the exact wall-time thresholds.
        // Accelerating real decoder preroll would spend its confirmation deadline
        // before a loaded scheduler can produce the new outputs.
        policy_time = match phase {
            Phase::Transition => unreachable!(),
            Phase::Stalled => policy_time + Duration::from_secs(1),
            Phase::Reopened {
                wall_time,
                policy_time,
            } => policy_time + wall_time.elapsed(),
        };
        match recovery.observe(policy_time, observation) {
            Action::Restart(permit) => {
                assert!(matches!(phase, Phase::Stalled));
                assert_eq!(permit.attempt(), 1);
                let wall_time = Instant::now();
                pipeline.take();
                pipeline = Some(Pipeline::start(&server.url)?);
                phase = Phase::Reopened {
                    wall_time,
                    policy_time,
                };
                continue;
            }
            Action::Failed => panic!("live output did not recover: {observation:?}"),
            Action::None => {}
        }
        if matches!(phase, Phase::Reopened { .. })
            && recovery.status() == Status::Idle
            && sample.video_output.is_some_and(|count| count > 1)
            && sample.audio_output.is_some_and(|count| count > 1)
        {
            break sample;
        }
    };
    assert!(restored_sample.video_output.is_some_and(|count| count > 1));
    assert!(restored_sample.audio_output.is_some_and(|count| count > 1));
    assert_eq!(server.requests.load(Ordering::Acquire), 2);
    Ok(())
}
