//! Raw source ownership is independent of playback. Pause/seek never stop reception.
mod file;
mod filesystem;
mod index;
pub mod limits;
pub use limits::{Limits, Policy};
mod source;
mod store;
#[cfg(test)]
mod tests;
use crate::transport::{framing::Framing, wire::TS_PACKET_SIZE};
use index::{Anchor, Index};
pub(super) use source::Input;
use std::{
    collections::VecDeque,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
pub use store::Retention;
use store::{READ_BYTES, ReadResult, Status, Store};

const PACKETS_PER_READ: usize = READ_BYTES / TS_PACKET_SIZE;
pub(super) const SEEK_PREROLL: Duration = Duration::from_secs(3);
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(15);
const RECONNECT_DELAY: Duration = Duration::from_secs(1);
const MAX_RECONNECTS: usize = 3;
const INPUT_WAIT: Duration = Duration::from_millis(10);
const FILE_SCAN_YIELD: Duration = Duration::from_millis(2);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("TS入力: {0}")]
    Io(#[from] std::io::Error),
    #[error("TS正規化: {0}")]
    Filter(#[from] cxx::Exception),
    #[error("TS入力の内部状態に異常があります")]
    Poisoned,
    #[error("シーク先のTSは保持範囲外です")]
    Expired,
    #[error("シーク先の番組情報をまだ取得できていません")]
    Unindexed,
    #[error("TS exploration was cancelled")]
    Cancelled,
}

struct FileIndex {
    index: Index,
    status: Status,
    survey: Option<file::Survey>,
    metadata_scan: Option<(Index, Anchor)>,
    priority: Arc<AtomicBool>,
}
enum Shared {
    File(Arc<Mutex<FileIndex>>),
    Live(Arc<Mutex<Store>>),
}
impl Clone for Shared {
    fn clone(&self) -> Self {
        match self {
            Self::File(index) => Self::File(index.clone()),
            Self::Live(store) => Self::Live(store.clone()),
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum Coverage {
    Complete,
    Estimated,
    Partial,
}
#[derive(Clone, Copy, Debug)]
struct Window {
    start: u64,
    end: u64,
    coverage: Coverage,
}
impl Shared {
    fn window(&self) -> Result<Option<Window>, Error> {
        let (start, end, coverage) = match self {
            Self::File(shared) => {
                let state = shared.lock().map_err(|_| Error::Poisoned)?;
                if let Status::Failed(message) = &state.status {
                    return Err(std::io::Error::other(message.clone()).into());
                }
                (
                    state
                        .index
                        .entries()
                        .front()
                        .map(|entry| entry.time_ns)
                        .or_else(|| state.survey.as_ref().map(file::Survey::start_ns)),
                    if matches!(state.status, Status::Ended) {
                        state.index.end_ns()
                    } else {
                        state
                            .survey
                            .as_ref()
                            .map(|survey| survey.end_ns)
                            .or_else(|| state.index.end_ns())
                    },
                    if matches!(state.status, Status::Ended) {
                        Coverage::Complete
                    } else if state.survey.is_some() {
                        Coverage::Estimated
                    } else {
                        Coverage::Partial
                    },
                )
            }
            Self::Live(shared) => {
                let store = shared.lock().map_err(|_| Error::Poisoned)?;
                (
                    store.index.entries().front().map(|entry| entry.time_ns),
                    store.index.end_ns(),
                    Coverage::Partial,
                )
            }
        };
        Ok(start.zip(end).map(|(start, end)| Window {
            start,
            end,
            coverage,
        }))
    }
    fn anchor(&self, target: u64) -> Result<Anchor, Error> {
        let choose = |index: &Index| {
            let first = index.entries().front().ok_or(Error::Unindexed)?;
            let end = index.end_ns().ok_or(Error::Unindexed)?;
            if target < first.time_ns || target >= end {
                return Err(Error::Expired);
            }
            // Live eviction continues during preroll. On a short window leave
            // half the distance to the oldest data available to the receiver.
            let preroll_ns = match self {
                Self::File(_) => SEEK_PREROLL.as_nanos() as u64,
                Self::Live(_) => (SEEK_PREROLL.as_nanos() as u64).min((target - first.time_ns) / 2),
            };
            let preroll = target.saturating_sub(preroll_ns);
            let anchor = index
                .entries()
                .iter()
                .rev()
                .find(|entry| entry.time_ns <= preroll)
                .unwrap_or(first)
                .clone();
            Ok(anchor)
        };
        match self {
            Self::File(shared) => choose(&shared.lock().map_err(|_| Error::Poisoned)?.index),
            Self::Live(shared) => choose(&shared.lock().map_err(|_| Error::Poisoned)?.index),
        }
    }
}
// Constructed only from this source's retained index, consumed by its reader.
struct SeekPlan<'a> {
    reader: &'a mut Reader,
    anchor: SeekAnchor,
}
impl SeekPlan<'_> {
    fn execute(self) -> Result<(), Error> {
        self.reader.pending_seek = Some(self.anchor);
        self.reader.pending.clear();
        Ok(())
    }
}

enum SeekAnchor {
    Indexed(Anchor),
    Search { target: u64, survey: file::Survey },
}

enum ReaderSource {
    File(File),
    Live(Arc<Mutex<Store>>),
}
struct Reader {
    source: ReaderSource,
    shared: Shared,
    offset: u64,
    framing: Framing,
    service: u16,
    filter: tsreadex::Filter,
    clock: Index,
    bootstrap: Vec<u8>,
    time_ns: u64,
    epoch: u64,
    pending: VecDeque<Output>,
    pending_seek: Option<SeekAnchor>,
}
impl Reader {
    fn prepare(&mut self, target: u64) -> Result<SeekPlan<'_>, Error> {
        let anchor = match self.shared.anchor(target) {
            Ok(anchor) => SeekAnchor::Indexed(anchor),
            Err(error) => {
                let Shared::File(shared) = &self.shared else {
                    return Err(error);
                };
                let state = shared.lock().map_err(|_| Error::Poisoned)?;
                let Some(survey) = &state.survey else {
                    return Err(error);
                };
                if target >= survey.end_ns {
                    return Err(error);
                }
                SeekAnchor::Search {
                    target,
                    survey: survey.clone(),
                }
            }
        };
        Ok(SeekPlan {
            reader: self,
            anchor,
        })
    }
    fn apply(&mut self, anchor: Anchor) -> Result<(), Error> {
        self.filter = tsreadex::Filter::new(self.service)?;
        self.clock = self.clock.reader();
        self.clock.seed(&anchor);
        self.bootstrap = (*anchor.bootstrap).clone();
        for packet in self.bootstrap.as_chunks::<TS_PACKET_SIZE>().0 {
            self.clock.packet(anchor.offset, packet);
        }
        self.offset = anchor.offset;
        self.time_ns = anchor.time_ns;
        self.epoch = anchor.epoch;
        self.pending.clear();
        // Continue SI acquisition independently of a paused decoder. The scan
        // owns another catalog cursor and can be replaced by the next seek.
        if self.clock.metadata_enabled()
            && let Shared::File(shared) = &self.shared
        {
            shared.lock().map_err(|_| Error::Poisoned)?.metadata_scan =
                Some((self.clock.reader(), anchor));
        }
        Ok(())
    }
    #[cfg(test)]
    fn next(&mut self) -> Result<Output, Error> {
        self.next_with_cancel(|| false)
    }
    fn next_with_cancel(&mut self, cancelled: impl Fn() -> bool) -> Result<Output, Error> {
        if let Some(seek) = self.pending_seek.take() {
            let anchor = match seek {
                SeekAnchor::Indexed(anchor) => {
                    self.clock.indexed();
                    anchor
                }
                SeekAnchor::Search { target, survey } => {
                    let _priority = if let Shared::File(shared) = &self.shared {
                        Some(file::Priority::new(
                            shared.lock().map_err(|_| Error::Poisoned)?.priority.clone(),
                        ))
                    } else {
                        None
                    };
                    let ReaderSource::File(file) = &mut self.source else {
                        return Err(Error::Unindexed);
                    };
                    let anchor =
                        survey.locate(file, self.framing, self.service, target, &cancelled)?;
                    self.clock.provisional();
                    anchor
                }
            };
            self.apply(anchor)?;
        }

        if let Some(output) = self.pending.pop_front() {
            return Ok(output);
        }
        let data = match &mut self.source {
            ReaderSource::File(file) => {
                file.seek(SeekFrom::Start(self.offset))?;
                let data = read_block(file, self.framing.stride() * PACKETS_PER_READ)?;
                if data.len() < TS_PACKET_SIZE {
                    ReadResult::End
                } else {
                    ReadResult::Data {
                        bytes: data,
                        reconnected: false,
                    }
                }
            }
            ReaderSource::Live(store) => store
                .lock()
                .map_err(|_| Error::Poisoned)?
                .read(self.offset)?,
        };
        match data {
            ReadResult::Data {
                bytes: data,
                reconnected,
            } => {
                if reconnected {
                    self.clock.discontinuity();
                    self.filter = tsreadex::Filter::new(self.service)?;
                }
                let mut bytes = self.filter.push(&std::mem::take(&mut self.bootstrap))?;
                let mut time_ns = self.time_ns;
                let mut discontinuity = false;
                for (offset, packet) in self.framing.packets(&data) {
                    if let Some(anchor) = self.clock.packet(self.offset + offset, packet) {
                        if !bytes.is_empty() {
                            self.pending.push_back(Output::Data {
                                bytes: std::mem::take(&mut bytes),
                                time_ns,
                                discontinuity,
                            });
                        }
                        time_ns = anchor.time_ns;
                        self.time_ns = time_ns;
                        discontinuity = self.epoch != anchor.epoch;
                        if discontinuity {
                            self.filter = tsreadex::Filter::new(self.service)?;
                            bytes.extend(self.filter.push(&anchor.bootstrap)?);
                        }
                        self.epoch = anchor.epoch;
                    }
                    if self.service == 0 && self.clock.service() != 0 {
                        self.service = self.clock.service();
                        self.filter = tsreadex::Filter::new(self.service)?;
                    }
                    bytes.extend(self.filter.push(packet)?);
                }
                self.clock.clear_entries();
                self.offset += data.len() as u64;
                if !bytes.is_empty() {
                    self.pending.push_back(Output::Data {
                        bytes,
                        time_ns,
                        discontinuity,
                    });
                }
                Ok(self.pending.pop_front().unwrap_or(Output::Awaiting))
            }
            ReadResult::Awaiting => Ok(Output::Awaiting),
            ReadResult::Expired => Ok(Output::Expired),
            ReadResult::End => Ok(Output::End),
        }
    }
}
enum Output {
    Data {
        bytes: Vec<u8>,
        time_ns: u64,
        discontinuity: bool,
    },
    Awaiting,
    Expired,
    End,
}

struct Cancellation(Arc<AtomicBool>);
impl Drop for Cancellation {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
enum Worker {
    File(Cancellation),
    Live(tokio::task::JoinHandle<()>),
}
impl Drop for Worker {
    fn drop(&mut self) {
        match self {
            Self::File(flag) => flag.0.store(true, Ordering::Release),
            Self::Live(task) => task.abort(),
        }
    }
}
fn file_reader(path: &Path, service: u16, programs: bool) -> Result<(Reader, Worker), Error> {
    let mut scan = File::open(path)?;
    let prefix = read_block(&mut scan, READ_BYTES)?;
    let framing = Framing::detect(&prefix).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "TS framing not found")
    })?;
    scan.seek(SeekFrom::Start(framing.offset()))?;
    let file = File::open(path)?;
    let shared = Arc::new(Mutex::new(FileIndex {
        index: Index::new(service, programs),
        status: Status::Receiving,
        survey: None,
        metadata_scan: None,
        priority: Arc::new(AtomicBool::new(false)),
    }));
    let cancellation = Cancellation(Arc::new(AtomicBool::new(false)));
    let cancelled = cancellation.0.clone();
    let indexed = shared.clone();
    std::thread::Builder::new()
        .name("ts-index".into())
        .spawn(move || {
            let result = (|| -> Result<(), Error> {
                // Discover endpoints first; do not wait for a NAS to stream the
                // entire recording before publishing a provisional seek range.
                let survey = file::Survey::discover(&mut scan, framing, service, programs, || {
                    cancelled.load(Ordering::Acquire)
                })?;
                indexed.lock().map_err(|_| Error::Poisoned)?.survey = survey;
                scan.seek(SeekFrom::Start(framing.offset()))?;
                let mut offset = framing.offset();
                while !cancelled.load(Ordering::Acquire) {
                    if indexed
                        .lock()
                        .map_err(|_| Error::Poisoned)?
                        .priority
                        .load(Ordering::Acquire)
                    {
                        std::thread::sleep(INPUT_WAIT);
                        continue;
                    }
                    let metadata_scan = indexed
                        .lock()
                        .map_err(|_| Error::Poisoned)?
                        .metadata_scan
                        .take();
                    if let Some((mut metadata, anchor)) = metadata_scan {
                        scan_metadata(
                            &mut scan,
                            framing,
                            &indexed,
                            &cancelled,
                            &mut metadata,
                            &anchor,
                        )?;
                        scan.seek(SeekFrom::Start(offset))?;
                    }
                    let bytes = read_block(&mut scan, framing.stride() * PACKETS_PER_READ)?;
                    if bytes.is_empty() {
                        break;
                    }
                    let mut state = indexed.lock().map_err(|_| Error::Poisoned)?;
                    for (within, packet) in framing.packets(&bytes) {
                        state.index.packet(offset + within, packet);
                    }
                    if state
                        .survey
                        .as_ref()
                        .is_some_and(|survey| !survey.agrees_with(&state.index))
                    {
                        state.survey = None;
                        state.index.clear_provisional();
                    }
                    offset += bytes.len() as u64;
                    state.index.compact_file();
                    drop(state);
                    // Limit background read pressure while playback/seek owns
                    // another handle to a slow network-mounted recording.
                    std::thread::sleep(FILE_SCAN_YIELD);
                }
                Ok(())
            })();
            if let Ok(mut state) = indexed.lock() {
                state.status = match result {
                    Ok(()) => Status::Ended,
                    Err(error) => Status::Failed(error.to_string()),
                };
            }
        })?;
    let clock = shared.lock().map_err(|_| Error::Poisoned)?.index.reader();
    let shared = Shared::File(shared);
    let reader = Reader {
        source: ReaderSource::File(file),
        shared,
        offset: framing.offset(),
        framing,
        service,
        filter: tsreadex::Filter::new(service)?,
        clock,
        bootstrap: Vec::new(),
        time_ns: 0,
        epoch: 0,
        pending: VecDeque::new(),
        pending_seek: None,
    };
    Ok((reader, Worker::File(cancellation)))
}

// Bounded lookahead covers sparse EIT/TOT even when output is paused after seek.
// A metadata read failure must not terminate the independent playback index.
fn scan_metadata(
    file: &mut File,
    framing: Framing,
    shared: &Mutex<FileIndex>,
    cancelled: &AtomicBool,
    index: &mut Index,
    anchor: &Anchor,
) -> Result<(), Error> {
    const MAX_SCAN_BYTES: u64 = 64 * 1024 * 1024;
    const MAX_SCAN_NS: u64 = 30_000_000_000;
    index.seed(anchor);
    for packet in anchor.bootstrap.as_chunks::<TS_PACKET_SIZE>().0 {
        index.packet(anchor.offset, packet);
    }
    if file.seek(SeekFrom::Start(anchor.offset)).is_err() {
        return Ok(());
    }
    let mut offset = anchor.offset;
    while offset.saturating_sub(anchor.offset) < MAX_SCAN_BYTES
        && !cancelled.load(Ordering::Acquire)
    {
        if shared
            .lock()
            .map_err(|_| Error::Poisoned)?
            .metadata_scan
            .is_some()
        {
            break;
        }
        let bytes = match read_block(file, framing.stride() * PACKETS_PER_READ) {
            Ok(bytes) if !bytes.is_empty() => bytes,
            Ok(_) | Err(_) => break,
        };
        for (within, packet) in framing.packets(&bytes) {
            index.packet(offset + within, packet);
        }
        offset += bytes.len() as u64;
        index.clear_entries();
        if index
            .end_ns()
            .is_some_and(|end| end >= anchor.time_ns.saturating_add(MAX_SCAN_NS))
        {
            break;
        }
        std::thread::sleep(FILE_SCAN_YIELD);
    }
    Ok(())
}

fn live_reader(
    uri: &str,
    service: u16,
    retention: Policy,
    programs: bool,
) -> Result<(Reader, Worker), Error> {
    static RUNTIME: std::sync::OnceLock<std::io::Result<tokio::runtime::Runtime>> =
        std::sync::OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
    });
    let runtime = runtime
        .as_ref()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let filter = tsreadex::Filter::new(service)?;
    let store = Arc::new(Mutex::new(Store::new(retention, service, programs)?));
    let receiving = store.clone();
    let uri = uri.to_owned();
    let task = runtime.spawn(async move {
        let result = receive(&uri, &receiving).await;
        if let Ok(mut store) = receiving.lock() {
            store.status = match result {
                Ok(()) => Status::Ended,
                Err(error) => Status::Failed(error),
            };
        }
    });
    let reader = Reader {
        source: ReaderSource::Live(store.clone()),
        shared: Shared::Live(store),
        offset: 0,
        framing: Framing::transport(),
        service,
        filter,
        clock: Index::new(service, false),
        bootstrap: Vec::new(),
        time_ns: 0,
        epoch: 0,
        pending: VecDeque::new(),
        pending_seek: None,
    };
    Ok((reader, Worker::Live(task)))
}
#[derive(Debug, thiserror::Error)]
enum ReceiveError {
    #[error("{0}")]
    Network(#[from] reqwest::Error),
    #[error("TS保持領域: {0}")]
    Storage(#[from] std::io::Error),
    #[error("TS保持領域の内部状態に異常があります")]
    Poisoned,
    #[error("配信の接続が終了しました")]
    Disconnected,
}
async fn receive(uri: &str, store: &Mutex<Store>) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(RECEIVE_TIMEOUT)
        .read_timeout(RECEIVE_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())?;
    let mut last_error = String::new();
    for attempt in 0..=MAX_RECONNECTS {
        if attempt != 0 {
            tokio::time::sleep(RECONNECT_DELAY).await;
            store.lock().map_err(|_| "TS store poisoned")?.reconnect();
        }
        let result = async {
            let mut response = client.get(uri).send().await?.error_for_status()?;
            let mut pending = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                // Retain less than one packet between reads; incoming HTTP chunks
                // are consumed in bounded slices before the next await.
                for bytes in chunk.chunks(READ_BYTES) {
                    pending.extend_from_slice(bytes);
                    let complete = pending.len() / TS_PACKET_SIZE * TS_PACKET_SIZE;
                    store
                        .lock()
                        .map_err(|_| ReceiveError::Poisoned)?
                        .append(&pending[..complete])?;
                    pending.drain(..complete);
                }
            }
            Err::<(), ReceiveError>(ReceiveError::Disconnected)
        }
        .await;
        match result {
            Err(error @ (ReceiveError::Storage(_) | ReceiveError::Poisoned)) => {
                return Err(error.to_string());
            }
            Err(ReceiveError::Network(error))
                if error
                    .status()
                    .is_some_and(|status| status.is_client_error()) =>
            {
                return Err(error.to_string());
            }
            Err(error @ (ReceiveError::Network(_) | ReceiveError::Disconnected)) => {
                last_error = error.to_string()
            }
            Ok(()) => return Ok(()),
        }
    }
    Err(last_error)
}

fn read_block(file: &mut impl Read, bytes: usize) -> std::io::Result<Vec<u8>> {
    let mut block = Vec::with_capacity(bytes);
    file.take(bytes as u64).read_to_end(&mut block)?;
    Ok(block)
}
