//! Retain six completed segments, without collecting every path in memory.
use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};
const KEEP: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Process {
    Alive,
    Dead,
    Unknown,
}

pub fn prune(directory: &Path) -> io::Result<usize> {
    prune_with(directory, process)
}

fn process(pid: u32) -> Process {
    #[cfg(target_os = "linux")]
    {
        // Missing/restricted procfs is not evidence that every process has exited.
        if !fs::metadata("/proc/self").is_ok_and(|metadata| metadata.is_dir()) {
            return Process::Unknown;
        }
        match fs::metadata(format!("/proc/{pid}")) {
            Ok(_) => Process::Alive,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Process::Dead,
            Err(_) => Process::Unknown,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        Process::Unknown
    }
}

fn owner(name: &str) -> Option<u32> {
    let name = name.strip_prefix("usage-")?.strip_suffix(".jsonl")?;
    let digits = name.strip_suffix(".previous").unwrap_or(name);
    let pid = digits.parse::<u32>().ok()?;
    (pid != 0 && digits == pid.to_string()).then_some(pid)
}

fn prune_with(directory: &Path, probe: impl Fn(u32) -> Process) -> io::Result<usize> {
    let mut newest: BinaryHeap<Reverse<(SystemTime, PathBuf, u32)>> =
        BinaryHeap::with_capacity(KEEP + 1);
    let mut removed = 0;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(owner) else {
            continue;
        };
        if probe(pid) != Process::Dead || !entry.file_type()?.is_file() {
            continue;
        }
        newest.push(Reverse((entry.metadata()?.modified()?, entry.path(), pid)));
        if newest.len() > KEEP {
            // len > KEEP guarantees an entry; pattern matching keeps cleanup fallible-free here.
            if let Some(Reverse((modified, path, pid))) = newest.pop() {
                // Recheck after other entries have been inspected. Skip replaced files and links.
                if probe(pid) != Process::Dead {
                    continue;
                }
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error),
                };
                if metadata.is_file() && metadata.modified()? == modified {
                    match fs::remove_file(path) {
                        Ok(()) => removed += 1,
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error),
                    }
                }
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_newest_six_completed_segments_and_leaves_other_files() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        for pid in 1..=20 {
            let path = dir.path().join(format!("usage-{pid}.jsonl"));
            let file = fs::File::create(path)?;
            file.set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(pid))?;
        }
        fs::write(dir.path().join("notes.jsonl"), "keep")?;
        fs::write(dir.path().join("usage-0.jsonl"), "keep")?;
        fs::create_dir(dir.path().join("usage-30.jsonl"))?;
        assert_eq!(prune_with(dir.path(), |_| Process::Dead)?, 14);
        for pid in 1..=20 {
            assert_eq!(
                dir.path().join(format!("usage-{pid}.jsonl")).exists(),
                pid >= 15
            );
        }
        assert!(dir.path().join("notes.jsonl").exists());
        assert!(dir.path().join("usage-0.jsonl").exists());
        assert!(dir.path().join("usage-30.jsonl").is_dir());
        Ok(())
    }
    #[test]
    fn live_and_unknown_owners_are_never_cleanup_candidates() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        for pid in 1..=20 {
            fs::write(
                dir.path().join(format!("usage-{pid}.previous.jsonl")),
                "keep",
            )?;
        }
        assert_eq!(
            prune_with(dir.path(), |pid| if pid % 2 == 0 {
                Process::Alive
            } else {
                Process::Unknown
            })?,
            0
        );
        assert_eq!(fs::read_dir(dir.path())?.count(), 20);
        #[cfg(target_os = "linux")]
        assert!(process(std::process::id()) == Process::Alive);
        Ok(())
    }
    #[test]
    fn rechecks_process_before_removing_a_queued_candidate() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        for pid in 1..=10 {
            let file = fs::File::create(dir.path().join(format!("usage-{pid}.jsonl")))?;
            file.set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(pid))?;
        }
        let observations = std::cell::Cell::new(0);
        prune_with(dir.path(), |pid| {
            if pid != 1 {
                return Process::Dead;
            }
            let count = observations.get();
            observations.set(count + 1);
            if count == 0 {
                Process::Dead
            } else {
                Process::Alive
            }
        })?;
        assert!(observations.get() >= 2);
        assert!(dir.path().join("usage-1.jsonl").exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_not_logs_to_prune() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let target = dir.path().join("keep");
        fs::write(&target, "keep")?;
        for pid in 1..=20 {
            std::os::unix::fs::symlink(&target, dir.path().join(format!("usage-{pid}.jsonl")))?;
        }
        assert_eq!(prune_with(dir.path(), |_| Process::Dead)?, 0);
        assert_eq!(fs::read_dir(dir.path())?.count(), 21);
        Ok(())
    }
}
