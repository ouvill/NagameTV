//! Passive, bounded local diagnostics. Never records programme/comment text.
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;
const QUEUE_SIZE: usize = 32;

#[derive(Default, Serialize)]
pub struct Snapshot {
    pub playing: bool,
    pub subtitles: bool,
    pub comments: bool,
    pub guide_open: bool,
    pub channels_open: bool,
    pub live_comments: usize,
    pub channel_count: usize,
    pub epg_programs: usize,
    pub epg_text_capacity_bytes: usize,
    pub comment_history_count: usize,
    pub comment_history_text_capacity_bytes: usize,
    pub subtitle_cells: usize,
    pub subtitle_pending: Option<usize>,
    pub epg_loading: bool,
}

#[derive(Serialize)]
struct Entry {
    schema: u8,
    pid: u32,
    version: &'static str,
    unix_ms: u128,
    elapsed_ms: u128,
    event: &'static str,
    snapshot: Snapshot,
}

enum Message {
    Sample(Entry),
    QtGc {
        unix_ms: u128,
        category: String,
        message: String,
    },
}

struct Sink {
    sender: mpsc::SyncSender<Message>,
    dropped: Arc<AtomicU64>,
}
static GC_SINK: OnceLock<Weak<Sink>> = OnceLock::new();

// Called by Qt's message handler on arbitrary threads; never performs file IO.
pub fn record_qt_gc(category: &str, message: &str) {
    if !matches!(
        category,
        "qt.qml.gc.statistics" | "qt.qml.gc.allocatorStats"
    ) {
        return;
    }
    if let Some(sink) = GC_SINK.get().and_then(Weak::upgrade) {
        // Bound each message, including UTF-8 and JSON escaping overhead.
        let message: String = message.chars().take(4096).collect();
        if sink
            .sender
            .try_send(Message::QtGc {
                unix_ms: unix_ms(),
                category: category.to_owned(),
                message,
            })
            .is_err()
        {
            sink.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub struct Recorder {
    sink: Arc<Sink>,
    dropped: Arc<AtomicU64>,
    started: Instant,
}

impl Recorder {
    pub fn start(directory: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&directory)?;
        prune_finished_logs(&directory)?;
        let path = directory.join(format!("usage-{}.jsonl", std::process::id()));
        let writer = RotatingWriter::new(path, MAX_LOG_BYTES)?;
        let (sender, receiver) = mpsc::sync_channel::<Message>(QUEUE_SIZE);
        let dropped = Arc::new(AtomicU64::new(0));
        let worker_dropped = dropped.clone();
        let sink = Arc::new(Sink {
            sender,
            dropped: dropped.clone(),
        });
        std::thread::Builder::new()
            .name("viewer-diagnostics".into())
            .spawn(move || {
                let mut writer = writer;
                for entry in receiver {
                    let measured_unix_ms = unix_ms();
                    let record = match entry {
                        Message::Sample(entry) => serde_json::json!({
                            "record": entry,
                            "measured_unix_ms": measured_unix_ms,
                            "process": process_memory(),
                            "allocator": allocator_memory(),
                            "dropped_records": worker_dropped.load(Ordering::Relaxed),
                        }),
                        Message::QtGc {
                            unix_ms,
                            category,
                            message,
                        } => serde_json::json!({
                            "kind": "qt_gc", "schema": 1, "pid": std::process::id(),
                            "unix_ms": unix_ms, "category": category, "message": message,
                            "dropped_records": worker_dropped.load(Ordering::Relaxed),
                        }),
                    };
                    if let Err(error) = writer.write(&record) {
                        tracing::warn!(%error, "Passive diagnostics stopped");
                        break;
                    }
                }
            })?;
        let _ = GC_SINK.set(Arc::downgrade(&sink));
        Ok(Self {
            sink,
            dropped,
            started: Instant::now(),
        })
    }

    pub fn record(&self, event: &'static str, snapshot: Snapshot) {
        let entry = Entry {
            schema: 1,
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION"),
            unix_ms: unix_ms(),
            elapsed_ms: self.started.elapsed().as_millis(),
            event,
            snapshot,
        };
        if self.sink.sender.try_send(Message::Sample(entry)).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis())
}

/// glibc accounting, not live application bytes: tcache and allocator metadata
/// affect these counters. Direct mmap (including QML's JS heap) is not covered.
#[derive(Serialize)]
struct AllocatorMemory {
    provider: &'static str,
    version: String,
    arena_bytes: usize,
    in_use_bytes: usize,
    free_bytes: usize,
    mmap_bytes: usize,
    mmap_regions: usize,
    releasable_top_bytes: usize,
}

fn allocator_memory() -> Option<AllocatorMemory> {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: mallinfo2 takes no pointers, returns a value, and locks arenas
        // internally. Sampling occurs only on the diagnostics worker.
        let info = unsafe { libc::mallinfo2() };
        // SAFETY: glibc returns a static, NUL-terminated version string.
        let version = unsafe { std::ffi::CStr::from_ptr(libc::gnu_get_libc_version()) }
            .to_string_lossy()
            .into_owned();
        Some(AllocatorMemory {
            provider: "glibc",
            version,
            arena_bytes: info.arena,
            in_use_bytes: info.uordblks,
            free_bytes: info.fordblks,
            mmap_bytes: info.hblkhd,
            mmap_regions: info.hblks,
            releasable_top_bytes: info.keepcost,
        })
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        None
    }
}

#[derive(Default, Serialize)]
struct ProcessMemory {
    rss_kib: Option<u64>,
    virtual_kib: Option<u64>,
    anonymous_kib: Option<u64>,
    lazy_free_kib: Option<u64>,
    pss_kib: Option<u64>,
    private_kib: Option<u64>,
    swap_kib: Option<u64>,
    threads: Option<u64>,
    fds: Option<usize>,
}

fn field(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        line.strip_prefix(key)?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

fn parse_memory(status: &str, smaps: &str) -> ProcessMemory {
    ProcessMemory {
        rss_kib: field(status, "VmRSS:"), // Same kernel counter used for htop RES.
        virtual_kib: field(status, "VmSize:"),
        anonymous_kib: field(smaps, "Anonymous:"),
        lazy_free_kib: field(smaps, "LazyFree:"),
        pss_kib: field(smaps, "Pss:"),
        private_kib: field(smaps, "Private_Clean:")
            .zip(field(smaps, "Private_Dirty:"))
            .map(|(clean, dirty)| clean + dirty),
        swap_kib: field(smaps, "Swap:"),
        threads: field(status, "Threads:"),
        fds: None,
    }
}

fn process_memory() -> ProcessMemory {
    // On unsupported systems or restricted procfs, unknown means null, not zero.
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let smaps = fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    let mut memory = parse_memory(&status, &smaps);
    memory.fds = fs::read_dir("/proc/self/fd")
        .ok()
        .map(|files| files.filter_map(Result::ok).count().saturating_sub(1));
    memory
}

struct RotatingWriter {
    path: PathBuf,
    file: File,
    bytes: u64,
    limit: u64,
}
impl RotatingWriter {
    fn new(path: PathBuf, limit: u64) -> io::Result<Self> {
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            path,
            file,
            bytes,
            limit,
        })
    }
    fn write(&mut self, value: &impl Serialize) -> io::Result<()> {
        let mut line = serde_json::to_vec(value).map_err(io::Error::other)?;
        line.push(b'\n');
        if line.len() as u64 > self.limit {
            return Err(io::Error::other("Diagnostic record exceeds file limit"));
        }
        if self.bytes + line.len() as u64 > self.limit {
            // Keep one previous segment per process; file copy works with open
            // file handles on all platforms, and runs only on the worker.
            self.file.flush()?;
            fs::copy(&self.path, self.path.with_extension("previous.jsonl"))?;
            self.file.set_len(0)?;
            self.bytes = 0;
        }
        self.file.write_all(&line)?;
        self.bytes += line.len() as u64;
        Ok(())
    }
}

fn prune_finished_logs(directory: &Path) -> io::Result<()> {
    let mut finished = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(pid) = name
            .strip_prefix("usage-")
            .and_then(|name| name.strip_suffix(".jsonl"))
            .and_then(|name| name.strip_suffix(".previous").or(Some(name)))
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        // Never unlink logs belonging to another live viewer process.
        if cfg!(target_os = "linux") && Path::new(&format!("/proc/{pid}")).exists() {
            continue;
        }
        let metadata = entry.metadata()?;
        if metadata.is_file() {
            finished.push((metadata.modified()?, entry.path()));
        }
    }
    finished.sort_by_key(|(modified, _)| *modified);
    let remove = finished.len().saturating_sub(6);
    for (_, path) in finished.into_iter().take(remove) {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test IO errors must fail the test; no assumption that filesystem IO is infallible.
    #[test]
    fn saturated_recorder_drops_without_blocking() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let dropped = Arc::new(AtomicU64::new(0));
        let recorder = Recorder {
            sink: Arc::new(Sink {
                sender,
                dropped: dropped.clone(),
            }),
            dropped: dropped.clone(),
            started: Instant::now(),
        };
        for _ in 0..1000 {
            recorder.record("sample", Snapshot::default());
        }
        assert_eq!(dropped.load(Ordering::Relaxed), 999);
        assert_eq!(receiver.try_iter().count(), 1);
    }

    #[test]
    fn parses_memory_and_preserves_missing_values() {
        let memory = parse_memory(
            "VmRSS:\t2048 kB\nThreads:\t41\n",
            "Pss: 1800 kB\nPrivate_Clean: 20 kB\nPrivate_Dirty: 1500 kB\nSwap: 0 kB\n",
        );
        assert_eq!(memory.rss_kib, Some(2048));
        assert_eq!(memory.pss_kib, Some(1800));
        assert_eq!(memory.private_kib, Some(1520));
        assert_eq!(memory.threads, Some(41));
        assert_eq!(parse_memory("", "").rss_kib, None);
    }
    #[test]
    fn recorder_writes_allocator_and_gc_records() -> io::Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "viewer-gc-test-{}-{}",
            std::process::id(),
            unix_ms()
        ));
        let recorder = Recorder::start(dir.clone())?;
        recorder.record("sample", Snapshot::default());
        record_qt_gc("unrelated.category", "must not be recorded");
        record_qt_gc("qt.qml.gc.statistics", "before \"GC\"\nafter GC");
        drop(recorder);
        let path = dir.join(format!("usage-{}.jsonl", std::process::id()));
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let text = fs::read_to_string(&path)?;
            let rows: Vec<serde_json::Value> = text
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();
            if rows.len() == 2 {
                assert_eq!(rows[0]["record"]["event"], "sample");
                #[cfg(all(target_os = "linux", target_env = "gnu"))]
                {
                    let allocator = &rows[0]["allocator"];
                    assert_eq!(allocator["provider"], "glibc");
                    let bytes = |key: &str| {
                        allocator[key].as_u64().ok_or_else(|| {
                            io::Error::other(format!("missing integer statistic: {key}"))
                        })
                    };
                    assert!(bytes("arena_bytes")? > 0);
                    assert_eq!(
                        bytes("arena_bytes")?,
                        bytes("in_use_bytes")? + bytes("free_bytes")?
                    );
                }
                assert_eq!(rows[1]["kind"], "qt_gc");
                assert_eq!(rows[1]["message"], "before \"GC\"\nafter GC");
                break;
            }
            assert!(
                Instant::now() < deadline,
                "diagnostic worker did not write records"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        fs::remove_dir_all(dir)
    }

    #[test]
    fn rotation_keeps_complete_json_lines_within_limit() -> io::Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "viewer-recording-test-{}-{}",
            std::process::id(),
            unix_ms()
        ));
        fs::create_dir_all(&dir)?;
        let path = dir.join("usage.jsonl");
        let mut writer = RotatingWriter::new(path.clone(), 80)?;
        for i in 0..20 {
            writer.write(&serde_json::json!({"sample":i}))?;
        }
        drop(writer);
        for file in [path.clone(), path.with_extension("previous.jsonl")] {
            let data = fs::read_to_string(file)?;
            assert!(data.len() <= 80);
            for line in data.lines() {
                serde_json::from_str::<serde_json::Value>(line).map_err(io::Error::other)?;
            }
        }
        fs::remove_dir_all(dir)
    }
}
