//! Disk retention owns one session lock. Stale sessions are removed on next use.
//! Linux uses XDG_CACHE_HOME (or ~/.cache), matching the application's XDG policy.
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

pub(super) struct Directory {
    directory: tempfile::TempDir,
    _lock: File,
}
impl Directory {
    pub fn new() -> std::io::Result<Self> {
        let root = cache_root()?;
        Self::in_root(&root)
    }
    pub(super) fn in_root(root: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(root)?;
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
        let directory = tempfile::Builder::new()
            .prefix("session-")
            .tempdir_in(root)?;
        let lock = File::options()
            .create_new(true)
            .read(true)
            .write(true)
            .open(directory.path().join("owner.lock"))?;
        lock.lock()?;
        drop(registry);
        Ok(Self {
            directory,
            _lock: lock,
        })
    }
    pub fn path(&self) -> &Path {
        self.directory.path()
    }
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
    Ok(root.join("mirakurun-viewer/timeshift"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_registration_never_removes_an_active_directory() -> std::io::Result<()> {
        const CONCURRENT_SESSIONS: usize = 8;
        let root = tempfile::tempdir()?;
        let registered = std::sync::Barrier::new(CONCURRENT_SESSIONS);
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..CONCURRENT_SESSIONS)
                .map(|_| {
                    scope.spawn(|| -> std::io::Result<()> {
                        let session = Directory::in_root(root.path());
                        registered.wait();
                        let session = session?;
                        assert!(session.path().join("owner.lock").is_file());
                        Ok(())
                    })
                })
                .collect();
            for worker in workers {
                worker.join().expect("session worker")?;
            }
            Ok(())
        })
    }

    #[test]
    fn cleanup_preserves_live_owners_and_removes_abandoned_sessions() -> std::io::Result<()> {
        let root = tempfile::tempdir()?;
        let live = Directory::in_root(root.path())?;
        let abandoned = tempfile::Builder::new()
            .prefix("session-")
            .tempdir_in(root.path())?
            .keep();
        File::create(abandoned.join("owner.lock"))?;
        File::create(abandoned.join("segment.ts"))?;
        let next = Directory::in_root(root.path())?;
        assert!(live.path().exists());
        assert!(next.path().exists());
        assert!(!abandoned.exists());
        Ok(())
    }
}
