use serde::Serialize;
use std::fs;

/// glibc accounting, not live application bytes: tcache and allocator metadata
/// affect these counters. Direct mmap (including QML's JS heap) is not covered.
#[derive(Serialize)]
pub struct AllocatorMemory {
    pub provider: &'static str,
    pub version: String,
    pub arena_bytes: usize,
    pub in_use_bytes: usize,
    pub free_bytes: usize,
    pub mmap_bytes: usize,
    pub mmap_regions: usize,
    pub releasable_top_bytes: usize,
}

pub fn allocator_memory() -> Option<AllocatorMemory> {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: mallinfo2 takes no pointers, returns a value, and locks arenas
        // internally. Call from the diagnostics worker, not the GUI thread.
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
pub struct ProcessMemory {
    pub rss_kib: Option<u64>,
    pub virtual_kib: Option<u64>,
    pub anonymous_kib: Option<u64>,
    pub lazy_free_kib: Option<u64>,
    pub pss_kib: Option<u64>,
    pub private_kib: Option<u64>,
    pub swap_kib: Option<u64>,
    pub threads: Option<u64>,
    pub fds: Option<usize>,
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
            .and_then(|(clean, dirty)| clean.checked_add(dirty)),
        swap_kib: field(smaps, "Swap:"),
        threads: field(status, "Threads:"),
        fds: None,
    }
}

pub fn process_memory() -> ProcessMemory {
    // On unsupported systems or restricted procfs, unknown means null, not zero.
    let status = bounded_text("/proc/self/status").unwrap_or_default();
    let smaps = bounded_text("/proc/self/smaps_rollup").unwrap_or_default();
    let mut memory = parse_memory(&status, &smaps);
    memory.fds = fs::read_dir("/proc/self/fd").ok().and_then(|mut files| {
        // If enumeration fails, a partial count must not appear authoritative.
        files
            .try_fold(0usize, |count, entry| entry.map(|_| count + 1))
            .ok()
            .map(|count| count.saturating_sub(1))
    });
    memory
}

fn bounded_text(path: impl AsRef<std::path::Path>) -> std::io::Result<String> {
    use std::io::Read;
    const LIMIT: u64 = 64 * 1024;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err(std::io::Error::other(
            "Process accounting file exceeds limit",
        ));
    }
    String::from_utf8(bytes).map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_invalid_and_overflowing_values_remain_unknown() {
        let memory = parse_memory(
            "VmRSS: 2048 kB\nThreads: 41\n",
            "Pss: 1800 kB\nPrivate_Clean: 20 kB\nPrivate_Dirty: 1500 kB\nSwap: 0 kB\n",
        );
        assert_eq!(memory.rss_kib, Some(2048));
        assert_eq!(memory.pss_kib, Some(1800));
        assert_eq!(memory.private_kib, Some(1520));
        assert_eq!(memory.threads, Some(41));
        assert_eq!(memory.swap_kib, Some(0));
        let memory = parse_memory(
            "VmRSS: invalid\n",
            "Private_Clean: 18446744073709551615 kB\nPrivate_Dirty: 1 kB\n",
        );
        assert_eq!(memory.rss_kib, None);
        assert_eq!(memory.private_kib, None);
        assert_eq!(memory.swap_kib, None);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn samples_current_process_without_display_or_playback() {
        let process = process_memory();
        assert!(process.rss_kib.is_some_and(|value| value > 0));
        assert!(process.threads.is_some_and(|value| value > 0));
        assert!(process.fds.is_some());
        // Restricted smaps may legitimately be unavailable; don't replace it with zero.
        #[cfg(target_env = "gnu")]
        {
            let allocator = allocator_memory();
            assert!(
                allocator
                    .as_ref()
                    .is_some_and(|value| value.provider == "glibc" && !value.version.is_empty())
            );
        }
    }

    #[test]
    fn process_input_is_bounded_and_invalid_utf8_is_rejected() -> std::io::Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        fs::write(file.path(), vec![b'a'; 64 * 1024 + 1])?;
        assert!(bounded_text(file.path()).is_err());
        fs::write(file.path(), [255])?;
        assert!(bounded_text(file.path()).is_err());
        fs::write(file.path(), "VmRSS: 1 kB")?;
        assert_eq!(bounded_text(file.path())?, "VmRSS: 1 kB");
        Ok(())
    }
}
