//! Bounded endpoint discovery and interpolation search for slow/random-access files.
//! Endpoints are provisional; observed playback ranges supply exact metadata.
use super::*;

const PROBE_BYTES: usize = 2 * 1024 * 1024;
const MAX_SEARCH_PROBES: usize = 12;
pub(super) const MAX_SEARCH_PREROLL: Duration = Duration::from_secs(8);
const MAX_CONTINUOUS_SPAN: Duration = Duration::from_secs(24 * 60 * 60);

pub(super) struct Metered<R> {
    inner: R,
    pub bytes: u64,
}
impl<R> Metered<R> {
    pub fn new(inner: R) -> Self {
        Self { inner, bytes: 0 }
    }
}
impl<R: Read> Read for Metered<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let count = self.inner.read(bytes)?;
        self.bytes += count as u64;
        Ok(count)
    }
}
impl<R: Seek> Seek for Metered<R> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

pub(super) struct Priority(Arc<AtomicBool>);
impl Priority {
    pub fn new(flag: Arc<AtomicBool>) -> Self {
        flag.store(true, Ordering::Release);
        Self(flag)
    }
}
impl Drop for Priority {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Clone)]
pub(super) struct Survey {
    first: Anchor,
    last: Anchor,
    pub end_ns: u64,
    programs: bool,
}
impl Survey {
    pub fn discover(
        file: &mut (impl Read + Seek),
        framing: Framing,
        service: u16,
        programs: bool,
        cancelled: impl Fn() -> bool,
    ) -> Result<Option<Self>, Error> {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let size = file.seek(SeekFrom::End(0))?;
        let Some(head) = probe(
            file,
            framing,
            service,
            programs,
            framing.offset(),
            &cancelled,
        )?
        else {
            return Ok(None);
        };
        let Some(first) = head.entries().front().cloned() else {
            return Ok(None);
        };
        let tail_offset = size
            .saturating_sub(PROBE_BYTES as u64)
            .max(framing.offset());
        let Some(tail) = probe(file, framing, service, programs, tail_offset, &cancelled)? else {
            return Ok(None);
        };
        let Some(raw_last) = tail.entries().back() else {
            return Ok(None);
        };
        if head
            .entries()
            .back()
            .is_some_and(|entry| entry.epoch != first.epoch)
            || tail
                .entries()
                .front()
                .is_some_and(|entry| entry.epoch != raw_last.epoch)
        {
            // Already observed a reset inside a sample; endpoints alone cannot
            // describe this recording's elapsed clock. Keep exact indexing.
            return Ok(None);
        }
        let last = raw_last.relative_to(&first);
        let end_ns = last.time_ns
            + tail
                .end_ns()
                .unwrap_or(raw_last.time_ns)
                .saturating_sub(raw_last.time_ns);
        if last.offset <= first.offset
            || end_ns <= first.time_ns
            || end_ns > MAX_CONTINUOUS_SPAN.as_nanos() as u64
        {
            return Ok(None);
        }
        Ok(Some(Self {
            first,
            last,
            end_ns,
            programs,
        }))
    }
    pub fn start_ns(&self) -> u64 {
        self.first.time_ns
    }
    pub fn agrees_with(&self, index: &Index) -> bool {
        index
            .entries()
            .back()
            .is_none_or(|anchor| anchor.epoch == self.first.epoch && anchor.time_ns <= self.end_ns)
    }
    pub fn locate(
        &self,
        file: &mut (impl Read + Seek),
        framing: Framing,
        service: u16,
        target: u64,
        cancelled: impl Fn() -> bool,
    ) -> Result<Anchor, Error> {
        let desired = target.saturating_sub(SEEK_PREROLL.as_nanos() as u64);
        let mut low = self.first.clone();
        let mut high = self.last.clone();
        if desired <= low.time_ns {
            return Ok(low);
        }
        for _ in 0..MAX_SEARCH_PROBES {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            let span = high.time_ns.saturating_sub(low.time_ns);
            if span == 0 || high.offset <= low.offset {
                break;
            }
            let fraction = u128::from(desired.saturating_sub(low.time_ns).min(span));
            let estimate = low.offset
                + (u128::from(high.offset - low.offset) * fraction / u128::from(span)) as u64;
            // Include tables and a preceding PCR, rather than probing only after
            // the target and falling back to a distant lower bound.
            let offset = estimate
                .saturating_sub((PROBE_BYTES / 2) as u64)
                .max(low.offset);
            let Some(sample) = probe(file, framing, service, self.programs, offset, &cancelled)?
            else {
                break;
            };
            let mut advanced = false;
            for raw in sample.entries() {
                let anchor = raw.relative_to(&self.first);
                if anchor.time_ns > self.end_ns {
                    continue;
                }
                if anchor.time_ns <= desired && anchor.offset >= low.offset {
                    advanced |= anchor.offset > low.offset;
                    low = anchor;
                } else if anchor.time_ns > desired && anchor.offset < high.offset {
                    advanced |= anchor.offset < high.offset;
                    high = anchor;
                }
            }
            if desired.saturating_sub(low.time_ns) <= MAX_SEARCH_PREROLL.as_nanos() as u64 {
                return Ok(low);
            }
            if !advanced {
                break;
            }
        }
        Err(Error::Unindexed)
    }
}
fn probe(
    file: &mut (impl Read + Seek),
    framing: Framing,
    service: u16,
    programs: bool,
    offset: u64,
    cancelled: &impl Fn() -> bool,
) -> Result<Option<Index>, Error> {
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let stride = framing.stride() as u64;
    let offset = framing.offset() + offset.saturating_sub(framing.offset()) / stride * stride;
    file.seek(SeekFrom::Start(offset))?;
    let mut index = Index::new(service, programs);
    let mut read = 0;
    while read < PROBE_BYTES && !cancelled() {
        let bytes = super::read_exploration(
            file,
            (PROBE_BYTES - read).min(framing.stride() * PACKETS_PER_READ),
            cancelled,
        )?;
        if bytes.is_empty() {
            break;
        }
        for (within, packet) in framing.packets(&bytes) {
            index.packet(offset + read as u64 + within, packet);
        }
        read += bytes.len();
    }
    if cancelled() {
        return Err(Error::Cancelled);
    }
    Ok((!index.entries().is_empty()).then_some(index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, io::Cursor};

    struct Metered {
        data: Cursor<Vec<u8>>,
        read_bytes: usize,
    }
    impl Read for Metered {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            let count = self.data.read(bytes)?;
            self.read_bytes += count;
            Ok(count)
        }
    }
    impl Seek for Metered {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            self.data.seek(position)
        }
    }
    #[test]
    fn endpoints_and_far_seek_use_bounded_reads_and_cancel_between_blocks() {
        use crate::transport::wire::{STUFFING_BYTE, SYNC_BYTE};
        const NULL_PID_HIGH: u8 = 0x1f;
        const NULL_PID_LOW: u8 = 0xff;
        const PAYLOAD_ONLY: u8 = 0x10;
        const PADDING_PACKETS: usize = 8;
        const TARGET: Duration = Duration::from_secs(45);
        let fixture = include_bytes!("../../../../tests/fixtures/recording-seek.ts");
        let mut padding = [STUFFING_BYTE; TS_PACKET_SIZE];
        padding[..4].copy_from_slice(&[SYNC_BYTE, NULL_PID_HIGH, NULL_PID_LOW, PAYLOAD_ONLY]);
        let mut bytes = Vec::new();
        for packet in fixture.as_chunks::<TS_PACKET_SIZE>().0 {
            bytes.extend_from_slice(packet);
            for _ in 0..PADDING_PACKETS {
                bytes.extend_from_slice(&padding);
            }
        }
        let mut source = Metered {
            data: Cursor::new(bytes),
            read_bytes: 0,
        };
        let framing = Framing::transport();
        let survey = Survey::discover(&mut source, framing, 1, true, || false)
            .unwrap()
            .unwrap();
        const ENDPOINTS: usize = 2;
        assert!(source.read_bytes <= ENDPOINTS * (PROBE_BYTES));
        assert!(source.read_bytes < source.data.get_ref().len());
        assert!(survey.end_ns > TARGET.as_nanos() as u64);
        source.read_bytes = 0;
        let anchor = survey
            .locate(&mut source, framing, 1, TARGET.as_nanos() as u64, || false)
            .unwrap();
        let desired = (TARGET - SEEK_PREROLL).as_nanos() as u64;
        assert!(anchor.time_ns <= desired);
        assert!(desired - anchor.time_ns <= MAX_SEARCH_PREROLL.as_nanos() as u64);
        assert!(source.read_bytes < source.data.get_ref().len());
        assert!(anchor.observation().is_some());
        source.read_bytes = 0;
        let checks = Cell::new(0);
        let result = survey.locate(&mut source, framing, 1, TARGET.as_nanos() as u64, || {
            checks.set(checks.get() + 1);
            checks.get() > ENDPOINTS
        });
        assert!(matches!(result, Err(Error::Cancelled)));
        assert!(source.read_bytes <= READ_BYTES);
        let reset = include_bytes!("../../../../tests/fixtures/recording-clock-reset.ts");
        let mut index = Index::new(1, false);
        for (number, packet) in reset.as_chunks::<TS_PACKET_SIZE>().0.iter().enumerate() {
            index.packet((number * TS_PACKET_SIZE) as u64, packet);
        }
        assert!(!survey.agrees_with(&index));
        assert!(
            Survey::discover(&mut Cursor::new(reset), framing, 1, false, || false)
                .unwrap()
                .is_none()
        );
    }
}

// Opening a network path may block. Only reader/metadata worker I/O reaches
// this transition; constructing the playback input performs no filesystem I/O.
pub(super) enum LocalFile {
    Pending(std::path::PathBuf),
    Open(std::fs::File),
}
impl LocalFile {
    pub fn new(path: &std::path::Path) -> Self {
        Self::Pending(path.to_owned())
    }
    fn file(&mut self) -> std::io::Result<&mut std::fs::File> {
        if let Self::Pending(path) = self {
            *self = Self::Open(std::fs::File::open(path)?);
        }
        match self {
            Self::Open(file) => Ok(file),
            Self::Pending(_) => unreachable!("opened file"),
        }
    }
}
impl Read for LocalFile {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.file()?.read(bytes)
    }
}
impl Seek for LocalFile {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.file()?.seek(position)
    }
}
