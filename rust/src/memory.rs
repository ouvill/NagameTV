//! Read-only allocator accounting; this does not trim or tune allocation policy.
pub fn record() {
    let unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // glibc returns a value snapshot; no pointers or allocator state are changed.
        let info = unsafe { libc::mallinfo2() };
        eprintln!(
            "ALLOC unix_ms={unix_ms} arena_used_kib={} arena_free_kib={} mmap_used_kib={}",
            info.uordblks / 1024,
            info.fordblks / 1024,
            info.hblkhd / 1024
        );
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    eprintln!("ALLOC unix_ms={unix_ms} accounting_unavailable");
}
