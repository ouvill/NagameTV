//! Process-wide startup allocation policy and read-only allocator accounting.

/// Call once from main, before initializing Qt, GStreamer or worker threads.
pub fn configure() -> Result<(), &'static str> {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // EPG有効で停止・再開を12回行うと、使用中の確保量はほぼ一定でも
        // glibcの空きarenaとRSSが約300MiB増えた。128KiB固定で増加が大幅に
        // 減ったため、動的なしきい値引き上げを止め、大きな一時バッファーを
        // 解放時にOSへ返しやすくする。EPGのどの確保が引き金かは未特定。
        // 詳細と比較条件: docs/allocator-investigation.md
        // 代替案はjemalloc/mimallocへの変更。ただしRustの#[global_allocator]
        // だけではQt/GStreamerのmallocは置換されず、プロセス全体の置換には
        // 別途リンク/プリロードと検証が必要。独自プールやGPUメモリーは対象外。
        // SAFETY: Valid glibc parameter/value, set before application threads start.
        let accepted = unsafe { libc::mallopt(libc::M_MMAP_THRESHOLD, 128 * 1024) };
        if accepted != 1 {
            return Err("glibc rejected M_MMAP_THRESHOLD=131072");
        }
        eprintln!("ALLOC policy=glibc mmap_threshold_bytes=131072 fixed=true");
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    eprintln!("ALLOC policy=system glibc_tuning_unavailable");
    Ok(())
}

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
