# Caller-side subtitle decoder mitigations

The native decoder is unchanged by the video lifecycle fix. Its C API currently
allocates caption text, regions and characters without checking every allocation
before copying (`third_party/libaribcaption/src/decoder/decoder_capi.cpp`). A Rust
check after `Decoder::decode` returns cannot protect those native operations.

Existing caller protections include the finite PES assembly budget (65,724 bytes
per assembled caption PES in `features/subtitles/pes.rs`), 188-byte ingestion
chunks, a 128-entry pending cue limit, and disabling poisoned parser state.
These bound specific Rust buffers; they do not bound all allocations inside the
native decoder or establish that every accepted input is safe.

Without modifying the decoder, the available next steps are:

1. Disable subtitles to avoid starting the subtitle decoding session. This
   sacrifices subtitles but removes this decoder from the playback path.
2. Apply input rate/aggregate budgets before decoding and output cell/text/size
   budgets before rendering. On a budget violation, drop the subtitle session
   and report the failure. This reduces resource pressure; limits applied after
   decoding cannot prevent a crash during decoding, and an ordinary error return
   does not cover a native access violation.
3. Run decoding in a separate Rust worker **process**, keeping the existing Rust
   binding inside that process. Bound IPC messages and the parent's queues, give
   the worker memory/time limits, and handle exit, timeout and malformed replies
   by disabling subtitles or restarting with a retry limit. Keep Qt and video
   playback in the parent. Process separation contains decoder address-space
   corruption; appropriate resource limits are also needed to contain resource
   exhaustion. A worker thread shares the application's address space.

`catch_unwind` is not crash isolation: it handles unwinding Rust panics, not all
aborting failures, and foreign exception handling is unspecified. See the
[Rust API contract](https://doc.rust-lang.org/std/panic/fn.catch_unwind.html).

Input/output budgets and process separation are proposals, not changes included
in this patch. If subtitles must remain enabled while leaving the native decoder
unchanged, a Rust worker process is the strongest of these caller-side options.
