//! Anonymous disk retention: one descriptor, recycled slots, no named stream data.
//! Linux O_TMPFILE and hole punching are required so even SIGKILL needs no cleanup.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use super::store::SEGMENT_BYTES;

// The slot map is private: a logical segment can only reuse physical bytes after
// remove() has successfully released them. Logical stream offsets never rewind.
pub(super) struct Buffer {
    file: File,
    slots: BTreeMap<u64, u64>,
    vacant: BTreeSet<u64>,
    extent: u64,
}
impl Buffer {
    pub fn new() -> io::Result<Self> {
        Self::in_root(&cache_root()?)
    }
    pub(super) fn in_root(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let file = anonymous_file(root)?;
        // Validate reclamation before a prepared buffer can replace live history.
        // No capacity-sized preallocation: the sparse file grows with reception.
        file.set_len(SEGMENT_BYTES as u64)?;
        release(&file, 0)?;
        file.set_len(0)?;
        cleanup_legacy_sessions(root)?;
        Ok(Self {
            file,
            slots: BTreeMap::new(),
            vacant: BTreeSet::new(),
            extent: 0,
        })
    }
    pub fn write(&mut self, start: u64, within: usize, bytes: &[u8]) -> io::Result<()> {
        check_range(within, bytes.len())?;
        let slot = *self.slots.entry(start).or_insert_with(|| {
            self.vacant.pop_first().unwrap_or_else(|| {
                let slot = self.extent;
                self.extent += SEGMENT_BYTES as u64;
                slot
            })
        });
        self.file.seek(SeekFrom::Start(slot + within as u64))?;
        self.file.write_all(bytes)
    }
    pub fn read(&mut self, start: u64, within: usize, count: usize) -> io::Result<Vec<u8>> {
        check_range(within, count)?;
        let slot = self.slot(start)?;
        self.file.seek(SeekFrom::Start(slot + within as u64))?;
        let mut bytes = vec![0; count];
        self.file.read_exact(&mut bytes)?;
        Ok(bytes)
    }
    pub fn remove(&mut self, start: u64) -> io::Result<()> {
        let slot = self.slot(start)?;
        release(&self.file, slot)?;
        self.slots.remove(&start);
        self.vacant.insert(slot);
        Ok(())
    }
    fn slot(&self, start: u64) -> io::Result<u64> {
        self.slots
            .get(&start)
            .copied()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "TS segment is not retained"))
    }
    #[cfg(all(test, target_os = "linux", target_env = "gnu"))]
    pub(super) fn deny_writes_for_test(&mut self) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        // Reopen the real inode read-only; no replacement IO implementation.
        self.file = File::open(format!("/proc/self/fd/{}", self.file.as_raw_fd()))?;
        Ok(())
    }
}
fn check_range(within: usize, count: usize) -> io::Result<()> {
    if within
        .checked_add(count)
        .is_none_or(|end| end > SEGMENT_BYTES)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "TS segment range overflow",
        ));
    }
    Ok(())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn anonymous_file(root: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    // Never fall back to create-then-unlink: that leaves a crash window. EXCL
    // prevents linking the inode later; CLOEXEC prevents inheritance across exec.
    File::options()
        .read(true)
        .write(true)
        .mode(libc::S_IRUSR | libc::S_IWUSR)
        .custom_flags(libc::O_TMPFILE | libc::O_EXCL | libc::O_CLOEXEC)
        .open(root)
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("匿名タイムシフトファイルを作成できません: {error}"),
            )
        })
}
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn release(file: &File, offset: u64) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let offset = libc::off_t::try_from(offset)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "TS file offset overflow"))?;
    // SAFETY: file owns a live fd; offset is nonnegative and size is positive.
    let result = unsafe {
        libc::fallocate(
            file.as_raw_fd(),
            libc::FALLOC_FL_PUNCH_HOLE | libc::FALLOC_FL_KEEP_SIZE,
            offset,
            SEGMENT_BYTES as libc::off_t,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        let error = io::Error::last_os_error();
        Err(io::Error::new(
            error.kind(),
            format!("タイムシフトのディスク領域を解放できません: {error}"),
        ))
    }
}
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn anonymous_file(_root: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "anonymous TS storage requires Linux/glibc",
    ))
}
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn release(_file: &File, _offset: u64) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "TS hole punching requires Linux/glibc",
    ))
}

// Migration only: old versions left named sessions behind. Keep their locking
// protocol so starting this version cannot delete another running version's data.
fn cleanup_legacy_sessions(root: &Path) -> io::Result<()> {
    // Serialize cleanup and registration: another process must not see a
    // newly created owner.lock before its session has acquired that lock.
    let registry = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("registry.lock"))?;
    registry.lock()?;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir()
            || !entry.file_name().to_string_lossy().starts_with("session-")
        {
            continue;
        }
        let marker = entry.path().join("owner.lock");
        if !marker
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.is_file())
        {
            continue;
        }
        let Ok(lock) = File::options().read(true).write(true).open(marker) else {
            continue;
        };
        if lock.try_lock().is_ok() {
            // Never remove another running instance's retained stream.
            if let Err(error) = fs::remove_dir_all(entry.path()) {
                tracing::debug!(%error, "Stale TS session cleanup failed");
            }
        }
    }
    Ok(())
}
fn cache_root() -> std::io::Result<PathBuf> {
    // This repository currently supports the XDG/Linux desktop. Keep the OS
    // policy here so the storage/reader contract has no path assumptions.
    let root = std::env::var_os("XDG_CACHE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cache directory is unavailable",
            )
        })?;
    Ok(root.join("nagametv/timeshift"))
}

#[cfg(all(test, target_os = "linux", target_env = "gnu"))]
mod tests {
    use super::*;
    use std::os::{fd::AsRawFd, unix::fs::MetadataExt};

    fn assert_no_stream_files(root: &Path) -> io::Result<()> {
        for entry in fs::read_dir(root)? {
            assert_eq!(entry?.file_name(), "registry.lock");
        }
        Ok(())
    }

    #[test]
    fn slots_recycle_without_corrupting_history_and_clear_removed_bytes() -> io::Result<()> {
        const RETAINED_SEGMENTS: u64 = 4;
        const RECEIVED_SEGMENTS: u64 = RETAINED_SEGMENTS * 8;
        let root = tempfile::tempdir()?;
        let mut buffer = Buffer::in_root(root.path())?;
        let fd = buffer.file.as_raw_fd();
        for start in 0..RECEIVED_SEGMENTS {
            if start >= RETAINED_SEGMENTS {
                buffer.remove(start - RETAINED_SEGMENTS)?;
            }
            let bytes = vec![start as u8; SEGMENT_BYTES];
            buffer.write(start, 0, &bytes)?;
            for retained in start.saturating_sub(RETAINED_SEGMENTS - 1)..=start {
                assert_eq!(
                    buffer.read(retained, 0, SEGMENT_BYTES)?,
                    vec![retained as u8; SEGMENT_BYTES]
                );
            }
            assert!(buffer.file.metadata()?.len() <= RETAINED_SEGMENTS * SEGMENT_BYTES as u64);
            assert_eq!(buffer.file.as_raw_fd(), fd);
            assert_eq!(buffer.file.metadata()?.nlink(), 0);
        }
        for start in RECEIVED_SEGMENTS - RETAINED_SEGMENTS..RECEIVED_SEGMENTS - 1 {
            let slot = buffer.slot(start)?;
            buffer.remove(start)?;
            assert!(buffer.read(start, 0, 1).is_err());
            // Check the hole's contents directly. Anonymous files on some
            // filesystems report a fixed st_blocks value even after fsync.
            buffer.file.seek(SeekFrom::Start(slot))?;
            let mut released = vec![0xff; SEGMENT_BYTES];
            buffer.file.read_exact(&mut released)?;
            assert!(released.iter().all(|byte| *byte == 0));
        }
        assert!(buffer.read(0, 0, 1).is_err());
        assert_eq!(
            buffer.read(RECEIVED_SEGMENTS - 1, 0, SEGMENT_BYTES)?,
            vec![(RECEIVED_SEGMENTS - 1) as u8; SEGMENT_BYTES]
        );
        assert_no_stream_files(root.path())?;
        drop(buffer);
        assert_no_stream_files(root.path())
    }

    #[test]
    fn failed_reclamation_keeps_the_slot_and_its_data() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let mut buffer = Buffer::in_root(root.path())?;
        let bytes = b"retained history";
        buffer.write(0, 0, bytes)?;
        buffer.deny_writes_for_test()?;
        assert!(buffer.remove(0).is_err());
        assert_eq!(buffer.read(0, 0, bytes.len())?, bytes);
        assert!(buffer.vacant.is_empty());
        Ok(())
    }

    #[test]
    fn concurrent_buffers_are_independent_and_legacy_cleanup_keeps_live_owners() -> io::Result<()> {
        const CONCURRENT_SESSIONS: usize = 8;
        let root = tempfile::tempdir()?;
        let live = tempfile::Builder::new()
            .prefix("session-")
            .tempdir_in(root.path())?;
        let lock = File::create(live.path().join("owner.lock"))?;
        lock.lock()?;
        let legacy_bytes = b"old version is still playing";
        fs::write(live.path().join("segment.ts"), legacy_bytes)?;
        let abandoned = tempfile::Builder::new()
            .prefix("session-")
            .tempdir_in(root.path())?
            .keep();
        File::create(abandoned.join("owner.lock"))?;
        File::create(abandoned.join("segment.ts"))?;
        let registered = std::sync::Barrier::new(CONCURRENT_SESSIONS);
        std::thread::scope(|scope| -> io::Result<()> {
            let workers: Vec<_> = (0..CONCURRENT_SESSIONS)
                .map(|session| {
                    let root = root.path();
                    let registered = &registered;
                    scope.spawn(move || -> io::Result<()> {
                        let buffer = Buffer::in_root(root);
                        registered.wait();
                        let mut buffer = buffer?;
                        let bytes = [session as u8];
                        buffer.write(0, 0, &bytes)?;
                        assert_eq!(buffer.read(0, 0, bytes.len())?, bytes);
                        Ok(())
                    })
                })
                .collect();
            for worker in workers {
                worker.join().expect("buffer worker")?;
            }
            Ok(())
        })?;
        assert_eq!(fs::read(live.path().join("segment.ts"))?, legacy_bytes);
        assert!(!abandoned.exists());
        // Other tests may fork between dropping our fd and the child's exec.
        // Explicitly end ownership so inherited CLOEXEC fds cannot delay this check.
        lock.unlock()?;
        drop(lock);
        let _next = Buffer::in_root(root.path())?;
        assert!(!live.path().exists());
        assert_no_stream_files(root.path())
    }

    // The same test is re-executed in a private subprocess. The parent tests both
    // normal close and SIGKILL, without changing limits/signals in the test runner.
    #[test]
    fn process_exit_releases_anonymous_history() -> io::Result<()> {
        use std::os::unix::process::ExitStatusExt;
        use std::{
            io::BufRead,
            process::{Command, Stdio},
            time::Duration,
        };
        const CHILD_ROOT: &str = "NAGAMETV_TEST_ANONYMOUS_ROOT";
        const CHILD_TEST: &str =
            "playback::input::filesystem::tests::process_exit_releases_anonymous_history";
        const READY: &str = "ANONYMOUS_READY ";
        const CHILD_TIMEOUT: Duration = Duration::from_secs(15);
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let mut buffer = Buffer::in_root(Path::new(&root))?;
            buffer.write(0, 0, &vec![0x5a; SEGMENT_BYTES])?;
            println!("{READY}{}", buffer.file.as_raw_fd());
            io::stdout().flush()?;
            let mut finish = [0];
            io::stdin().read_exact(&mut finish)?;
            drop(buffer);
            return Ok(());
        }
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        for kill in [false, true] {
            let root = tempfile::tempdir()?;
            let mut child = Child(
                Command::new(std::env::current_exe()?)
                    .args(["--exact", CHILD_TEST, "--nocapture"])
                    .env(CHILD_ROOT, root.path())
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()?,
            );
            let stdout = child.0.stdout.take().unwrap();
            let (sender, receiver) = std::sync::mpsc::channel();
            let reader = std::thread::spawn(move || {
                for line in io::BufReader::new(stdout).lines() {
                    let line = line.expect("child stdout");
                    if let Some((_, fd)) = line.split_once(READY) {
                        let _ = sender.send(fd.to_owned());
                    }
                }
            });
            let fd = receiver
                .recv_timeout(CHILD_TIMEOUT)
                .expect("child must create anonymous history");
            let proc_file = PathBuf::from(format!("/proc/{}/fd/{fd}", child.0.id()));
            let metadata = fs::metadata(&proc_file)?;
            assert_eq!(metadata.nlink(), 0);
            assert_eq!(metadata.dev(), fs::metadata(root.path())?.dev());
            assert!(metadata.blocks() > 0);
            assert_no_stream_files(root.path())?;
            if kill {
                child.0.kill()?;
            } else {
                child.0.stdin.take().unwrap().write_all(&[0])?;
            }
            let deadline = std::time::Instant::now() + CHILD_TIMEOUT;
            let status = loop {
                if let Some(status) = child.0.try_wait()? {
                    break status;
                }
                assert!(std::time::Instant::now() < deadline, "child did not exit");
                std::thread::sleep(Duration::from_millis(10));
            };
            if kill {
                assert_eq!(status.signal(), Some(libc::SIGKILL));
            } else {
                assert!(status.success());
            }
            reader.join().expect("child output reader");
            assert!(!proc_file.exists());
            assert_no_stream_files(root.path())?;
        }
        Ok(())
    }
}
