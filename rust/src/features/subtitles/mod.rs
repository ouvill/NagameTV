mod decoder;
mod gst_clock;
mod model;
mod timing;
pub(crate) use gst_clock::SubtitleClock;
pub(crate) use timing::SubtitleUpdate;

mod pes;
pub use model::SubtitleCue;
pub(crate) use pes::CaptionDecoder;
mod transport;

use crate::channels::BroadcastService;
use crate::features::subscriptions::Subscriptions;
use gstreamer::{self as gst, prelude::*};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Playback unavailable")]
    PlaybackUnavailable,
    #[error("Missing playback bin")]
    MissingBin,
    #[error("Missing playback bus")]
    MissingBus,
    #[error("放送サービス情報がないため字幕を開始できません")]
    MissingService,
    #[error("字幕デコーダーを初期化できません")]
    DecoderUnavailable,
}

fn parser_for(service: Option<BroadcastService>) -> Result<transport::TransportParser, Error> {
    let service = service.ok_or(Error::MissingService)?;
    let mut parser = transport::TransportParser::new(true);
    // Endpoint IDs identify HTTP resources; PAT uses the explicit broadcast serviceId.
    parser.select_service(service.service_id);
    if !parser.decoder_available() {
        return Err(Error::DecoderUnavailable);
    }
    Ok(parser)
}

/// A single playback generation. Construct before PLAYING; drop after READY.
pub struct Session {
    clock: SubtitleClock,
    subscriptions: Subscriptions,
    bus: gst::Bus,
    decoded: Arc<AtomicU64>,
}
impl Session {
    pub fn start(playbin: &gst::Element, service: Option<BroadcastService>) -> Result<Self, Error> {
        // Reject missing metadata before attaching any callbacks or probes.
        let parser = parser_for(service)?;
        let bin = playbin
            .downcast_ref::<gst::Bin>()
            .ok_or(Error::MissingBin)?;
        let bus = playbin.bus().ok_or(Error::MissingBus)?;
        let parser = Arc::new(Mutex::new(parser));
        let clock = SubtitleClock::default();
        let subscriptions = clock.attach(bin);
        let registrations = subscriptions.clone();
        let publish = clock.clone();
        let decoded = Arc::new(AtomicU64::new(0));
        let count = decoded.clone();
        let id = playbin.connect("source-setup", false, move |values| {
            if let Some(source) = values
                .get(1)
                .and_then(|value| value.get::<gst::Element>().ok())
                && let Some(pad) = source.static_pad("src")
            {
                let parser = parser.clone();
                let count = count.clone();
                let publish = publish.clone();
                let id = pad.add_probe(
                    gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST,
                    move |_, info| {
                        let mut parser = parser.lock().unwrap();
                        let mut consume = |buffer: &gst::BufferRef| {
                            if let Ok(bytes) = buffer.map_readable() {
                                // Bound temporary assembly even if upstream hands us a large buffer.
                                for chunk in bytes.as_slice().chunks(188) {
                                    let cues = parser.push(chunk);
                                    count.fetch_add(cues.len() as u64, Ordering::Relaxed);
                                    publish.push(cues);
                                }
                            }
                        };
                        if let Some(buffer) = info.buffer() {
                            consume(buffer);
                        }
                        if let Some(list) = info.buffer_list() {
                            for buffer in list.iter() {
                                consume(buffer);
                            }
                        }
                        gst::PadProbeReturn::Ok
                    },
                );
                registrations.probe(&pad, id);
            }
            None
        });
        subscriptions.signal(playbin, id);
        Ok(Self {
            clock,
            subscriptions,
            bus,
            decoded,
        })
    }
    pub fn poll(&self, position: Option<gst::ClockTime>) -> SubtitleUpdate {
        self.clock.poll(position)
    }
    pub fn counters(&self) -> (usize, usize, u64) {
        (
            self.subscriptions.count() + 1,
            self.clock.pending_count().unwrap_or(0),
            self.decoded.load(Ordering::Relaxed),
        )
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.subscriptions.close();
        self.bus.unset_sync_handler();
        self.clock.set_enabled(false);
    }
}
