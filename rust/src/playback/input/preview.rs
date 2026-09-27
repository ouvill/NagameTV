//! Independent, bounded thumbnail cursors. Never move the playback reader or
//! publish metadata observed while looking for a preview.
use super::*;

const MAX_PREVIEW_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub(in crate::playback) struct Source {
    origin: Origin,
    framing: Framing,
    service: u16,
}

#[derive(Clone)]
enum Origin {
    File {
        index: Arc<Mutex<FileIndex>>,
        source: super::super::recording::source::Source,
    },
    Live(Arc<Mutex<Store>>),
}

impl Source {
    pub(super) fn new(
        reader: &Reader,
        file: Option<super::super::recording::source::Source>,
    ) -> Result<Self, Error> {
        let origin = match (&reader.shared, file) {
            (Shared::File(index), Some(source)) => Origin::File {
                index: index.clone(),
                source,
            },
            (Shared::Live(store), None) => Origin::Live(store.clone()),
            _ => return Err(Error::Unindexed),
        };
        Ok(Self {
            origin,
            framing: reader.framing,
            service: reader.service,
        })
    }

    pub(in crate::playback) fn open(
        &self,
        target: u64,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Cursor, Error> {
        let (shared, mut source) = match &self.origin {
            Origin::File { index, source } => (
                Shared::File(index.clone()),
                ReaderSource::File(source.reader(cancelled.clone())),
            ),
            Origin::Live(store) => (
                Shared::Live(store.clone()),
                ReaderSource::Live(store.clone()),
            ),
        };
        let anchor = match shared.anchor(target) {
            Ok(anchor) => anchor,
            Err(error @ (Error::Unindexed | Error::Expired)) => {
                let Shared::File(shared) = &shared else {
                    return Err(error);
                };
                let survey = shared
                    .lock()
                    .map_err(|_| Error::Poisoned)?
                    .survey
                    .clone()
                    .ok_or(Error::Unindexed)?;
                if target < survey.start_ns() || target >= survey.end_ns {
                    return Err(Error::Expired);
                }
                let ReaderSource::File(file) = &mut source else {
                    return Err(Error::Unindexed);
                };
                let deadline = Instant::now() + EXPLORATION_TIMEOUT;
                let result = survey.locate(file, self.framing, self.service, target, || {
                    cancelled.load(Ordering::Acquire) || Instant::now() >= deadline
                });
                match result {
                    Err(Error::Cancelled) if !cancelled.load(Ordering::Acquire) => {
                        return Err(Error::ExplorationTimedOut);
                    }
                    result => result?,
                }
            }
            Err(error) => return Err(error),
        };
        Ok(Cursor {
            source,
            offset: anchor.offset,
            framing: self.framing,
            filter: tsreadex::Filter::new(self.service)?,
            bootstrap: Some(anchor.bootstrap),
            remaining: MAX_PREVIEW_BYTES,
            cancelled,
        })
    }
}

// Only Source::open can construct a cursor, after checking this source's index.
pub(in crate::playback) struct Cursor {
    source: ReaderSource,
    offset: u64,
    framing: Framing,
    filter: tsreadex::Filter,
    bootstrap: Option<Arc<Vec<u8>>>,
    remaining: usize,
    cancelled: Arc<AtomicBool>,
}

impl Cursor {
    pub(in crate::playback) fn next(&mut self) -> Result<Option<Vec<u8>>, Error> {
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(Error::Cancelled);
            }
            let read_bytes = self.remaining.min(self.framing.stride() * PACKETS_PER_READ)
                / self.framing.stride()
                * self.framing.stride();
            if read_bytes == 0 {
                return Ok(None);
            }
            let mut output = Vec::new();
            if let Some(bootstrap) = self.bootstrap.take() {
                output.extend_from_slice(self.filter.push(&bootstrap)?);
            }
            let bytes = match &mut self.source {
                ReaderSource::File(file) => {
                    file.seek(SeekFrom::Start(self.offset))?;
                    store::ReadBytes::Owned(read_block(file, read_bytes)?)
                }
                ReaderSource::Live(store) => match store
                    .lock()
                    .map_err(|_| Error::Poisoned)?
                    .read(self.offset)?
                {
                    ReadResult::Data {
                        bytes,
                        reconnected: false,
                    } => bytes,
                    ReadResult::Data {
                        reconnected: true, ..
                    }
                    | ReadResult::Expired => return Err(Error::Expired),
                    ReadResult::Awaiting | ReadResult::End => return Ok(None),
                },
            };
            let bytes = bytes.as_ref();
            let bytes = &bytes[..bytes.len().min(read_bytes)];
            if bytes.is_empty() {
                return Ok(None);
            }
            self.offset += bytes.len() as u64;
            self.remaining = self.remaining.saturating_sub(bytes.len());
            for (_, packet) in self.framing.packets(bytes) {
                output.extend_from_slice(self.filter.push(packet)?);
            }
            if !output.is_empty() {
                return Ok(Some(output));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::preview::{Controller, Source as PreviewSource};

    fn image(source: Source, target: f64) -> String {
        let mut controller = Controller::default();
        controller.request(PreviewSource::transport(1, source), target);
        let deadline = Instant::now() + Duration::from_secs(10);
        while controller.image().is_empty() && Instant::now() < deadline {
            controller.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            controller
                .image()
                .starts_with("data:image/png;base64,iVBOR"),
            "TS preview missing"
        );
        controller.image().to_owned()
    }

    #[test]
    fn recording_preview_does_not_move_playback_cursor() -> Result<(), Box<dyn std::error::Error>> {
        gstreamer::init()?;
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/recording-seek.ts");
        let (reader, _worker) = file_reader(&path, 1, false)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !reader
            .shared
            .window()?
            .is_some_and(|window| window.end > 50_000_000_000)
        {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let offset = reader.offset;
        let source = Source::new(
            &reader,
            Some(crate::playback::recording::source::Source::Local(path)),
        )?;
        let first = image(source.clone(), 10_000.0);
        let second = image(source, 45_000.0);
        assert_ne!(
            first, second,
            "different positions must decode different images"
        );
        assert_eq!(reader.offset, offset);
        Ok(())
    }

    #[test]
    fn retained_live_preview_uses_independent_cursor() -> Result<(), Box<dyn std::error::Error>> {
        gstreamer::init()?;
        let mut store = Store::new(Policy::new(Retention::Memory, Limits::default()), 1, false)?;
        store.append(include_bytes!(
            "../../../../tests/fixtures/recording-seek.ts"
        ))?;
        let source = Source {
            origin: Origin::Live(Arc::new(Mutex::new(store))),
            framing: Framing::transport(),
            service: 1,
        };
        image(source.clone(), 30_000.0);
        assert!(matches!(
            source.open(u64::MAX, Arc::new(AtomicBool::new(false))),
            Err(Error::Expired)
        ));
        Ok(())
    }
}
