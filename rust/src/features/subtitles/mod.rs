mod decoder;
#[cfg(test)]
#[path = "../../../crates/libaribcaption/tests/fixtures/sample.rs"]
mod fixture;
mod gst_clock;
mod ingest;
mod model;
mod timing;
pub(crate) use gst_clock::SubtitleClock;
pub(crate) use timing::SubtitleUpdate;

mod pes;
pub use model::SubtitleCue;
pub(crate) use pes::CaptionDecoder;
mod selection;
#[cfg(test)]
mod stream_selection_tests;
pub(crate) mod transport;
use crate::transport::wire;

use crate::channels::BroadcastService;
use crate::features::subscriptions::Subscriptions;
use gstreamer::{self as gst, prelude::*};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Missing playback bin")]
    MissingBin,
    #[error("Missing playback bus")]
    MissingBus,
    #[error("放送サービス情報がないため字幕を開始できません")]
    MissingService,
    #[error("字幕デコーダーを初期化できません")]
    DecoderUnavailable,
    #[error("字幕解析の内部状態に異常があります。停止してから再生し直してください")]
    ParserPoisoned,
    #[error("字幕の時刻対応に異常があります。停止してから再生し直してください")]
    ClockPoisoned,
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
    ingest: Arc<ingest::Ingest>,
}
impl Session {
    pub fn start(playbin: &gst::Element, service: Option<BroadcastService>) -> Result<Self, Error> {
        // Reject missing metadata before attaching any callbacks or probes.
        let parser = parser_for(service)?;
        Self::with_parser(playbin, parser)
    }
    pub fn start_recording(
        playbin: &gst::Element,
        service: u16,
        subtitles: bool,
    ) -> Result<Self, Error> {
        let mut parser = transport::TransportParser::new(subtitles);
        parser.select_service(service);
        if subtitles && !parser.decoder_available() {
            return Err(Error::DecoderUnavailable);
        }
        Self::with_parser(playbin, parser)
    }
    fn with_parser(
        playbin: &gst::Element,
        parser: transport::TransportParser,
    ) -> Result<Self, Error> {
        let bin = playbin
            .downcast_ref::<gst::Bin>()
            .ok_or(Error::MissingBin)?;
        let bus = playbin.bus().ok_or(Error::MissingBus)?;
        let clock = SubtitleClock::default();
        let subscriptions = clock.attach(bin);
        let registrations = subscriptions.clone();
        let ingest = Arc::new(ingest::Ingest::new(parser, clock.clone()));
        let source_ingest = ingest.clone();
        let id = playbin.connect("source-setup", false, move |values| {
            if let Some(source) = values
                .get(1)
                .and_then(|value| value.get::<gst::Element>().ok())
                && let Some(pad) = source.static_pad("src")
            {
                let ingest = source_ingest.clone();
                let id = pad.add_probe(
                    gst::PadProbeType::BUFFER
                        | gst::PadProbeType::BUFFER_LIST
                        | gst::PadProbeType::EVENT_DOWNSTREAM
                        | gst::PadProbeType::EVENT_UPSTREAM
                        | gst::PadProbeType::EVENT_FLUSH,
                    move |_, info| {
                        if let Some(event) = info.event() {
                            ingest.event(event);
                        }
                        ingest.consume(
                            info.buffer()
                                .map(|buffer| buffer.as_ref())
                                .into_iter()
                                .chain(info.buffer_list().into_iter().flat_map(|list| list.iter())),
                        );
                        // Keep the registered probe until READY, when Session
                        // releases it. Returning Remove here would leave a stale ID.
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
            ingest,
        })
    }
    pub fn poll(&self, position: Option<gst::ClockTime>) -> Result<SubtitleUpdate, Error> {
        self.ingest.check()?;
        self.clock.poll(position)
    }
    pub fn pending_diagnostic(&self) -> Option<usize> {
        self.clock.pending_count()
    }
    pub fn counters(&self) -> (usize, usize, u64) {
        (
            self.subscriptions.count() + 1,
            self.clock.pending_count().unwrap_or(0),
            self.ingest.decoded(),
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
