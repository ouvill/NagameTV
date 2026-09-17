//! Streaming-thread subtitle ingestion and a bounded failure notification to the UI.
use super::{Error, SubtitleClock, transport::TransportParser};
use gstreamer as gst;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

pub(super) struct Ingest {
    parser: Mutex<Input>,
    clock: SubtitleClock,
    failed: AtomicBool,
    decoded: AtomicU64,
}

struct Input {
    parser: TransportParser,
    flow: Flow,
}

enum Flow {
    Reading { next_offset: Option<u64> },
    Flushing,
}

impl Ingest {
    pub fn new(parser: TransportParser, clock: SubtitleClock) -> Self {
        Self {
            parser: Mutex::new(Input {
                parser,
                flow: Flow::Reading { next_offset: None },
            }),
            clock,
            failed: AtomicBool::new(false),
            decoded: AtomicU64::new(0),
        }
    }

    pub fn check(&self) -> Result<(), Error> {
        if self.failed.load(Ordering::Relaxed) {
            Err(Error::ParserPoisoned)
        } else {
            self.clock.check()
        }
    }

    pub fn decoded(&self) -> u64 {
        self.decoded.load(Ordering::Relaxed)
    }

    pub fn event(&self, event: &gst::EventRef) {
        if let Ok(mut input) = self.parser.lock() {
            match event.view() {
                gst::EventView::FlushStart(_) => {
                    input.parser.discontinuity();
                    input.flow = Flow::Flushing;
                    self.clock.reset();
                }
                gst::EventView::FlushStop(_) | gst::EventView::StreamStart(_) => {
                    input.parser.discontinuity();
                    input.flow = Flow::Reading { next_offset: None };
                    self.clock.reset();
                }
                _ => {}
            }
        }
    }

    pub fn consume<'a>(&self, buffers: impl IntoIterator<Item = &'a gst::BufferRef>) {
        if self.check().is_err() {
            return;
        }
        let Ok(mut input) = self.parser.lock() else {
            // Unlike the subscription ledger, partially decoded stream state
            // cannot be trusted after poisoning. Never recover or reuse it.
            // This latch publishes no other data, so Relaxed ordering suffices.
            self.failed.store(true, Ordering::Relaxed);
            self.clock.set_enabled(false);
            return;
        };
        for buffer in buffers {
            let Flow::Reading { next_offset } = input.flow else {
                continue;
            };
            let offset = (buffer.offset() != u64::MAX).then_some(buffer.offset());
            if buffer.flags().contains(gst::BufferFlags::DISCONT)
                || next_offset
                    .zip(offset)
                    .is_some_and(|(expected, actual)| expected != actual)
            {
                input.parser.discontinuity();
                self.clock.reset();
            }
            input.flow = Flow::Reading {
                next_offset: offset.and_then(|offset| offset.checked_add(buffer.size() as u64)),
            };
            if let Ok(bytes) = buffer.map_readable() {
                // Bound temporary assembly even for large upstream buffers.
                for chunk in bytes.as_slice().chunks(super::wire::TS_PACKET_SIZE) {
                    let cues = input.parser.push(chunk);
                    if input.parser.take_caption_reset() {
                        self.clock.clear_captions();
                    }
                    self.decoded.fetch_add(cues.len() as u64, Ordering::Relaxed);
                    self.clock.push(cues);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // unwrap in tests asserts fixture setup, not infallible production IO.
    use super::*;
    use crate::features::subtitles::{SubtitleCue, SubtitleUpdate};
    use std::sync::Arc;

    #[test]
    fn poison_stops_ingestion_clears_pending_cues_and_requires_a_new_generation() {
        // Memory-backed buffers only; no playback devices are constructed.
        gst::init().unwrap();
        let clock = SubtitleClock::default();
        let ingest = Arc::new(Ingest::new(TransportParser::new(false), clock.clone()));
        let buffer = gst::Buffer::from_slice(vec![0x47; 187]);
        ingest.consume([buffer.as_ref()]);
        assert!(ingest.check().is_ok());
        clock.push(vec![SubtitleCue::clear(100)]);
        assert_eq!(clock.pending_count(), Some(1));
        let other = ingest.clone();
        assert!(
            std::thread::spawn(move || {
                let _guard = other.parser.lock().unwrap();
                panic!("inject poisoned parser state");
            })
            .join()
            .is_err()
        );
        ingest.consume([buffer.as_ref()]);
        assert!(matches!(ingest.check(), Err(Error::ParserPoisoned)));
        assert_eq!(clock.pending_count(), Some(0));
        assert!(matches!(clock.poll(None).unwrap(), SubtitleUpdate::Clear));
        ingest.consume([buffer.as_ref()]);
        assert!(ingest.parser.is_poisoned());
        assert!(matches!(ingest.check(), Err(Error::ParserPoisoned)));
        assert_eq!(ingest.decoded(), 0);
        let session = super::super::Session {
            clock,
            subscriptions: crate::features::subscriptions::Subscriptions::default(),
            bus: gst::Bus::new(),
            ingest,
        };
        assert!(matches!(session.poll(None), Err(Error::ParserPoisoned)));
        drop(session);
        let next = Ingest::new(TransportParser::new(false), SubtitleClock::default());
        next.consume([buffer.as_ref()]);
        assert!(next.check().is_ok());
    }
}
