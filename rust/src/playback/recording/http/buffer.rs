//! One playback reader, one joined worker and one bounded contiguous window.
use super::{CANCEL_POLL, Error, RANGE_BYTES, Verified, stream};
use std::{
    collections::VecDeque,
    io,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const INITIAL_BYTES: usize = 2 * 1024 * 1024;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const AHEAD_SECONDS: u128 = 10;

enum Phase {
    Ready,
    Retrying,
    Superseded,
    Failed(Arc<Error>),
}
struct Block {
    start: u64,
    bytes: Vec<u8>,
}
impl Block {
    fn end(&self) -> u64 {
        self.start + self.bytes.len() as u64
    }
}
struct State {
    wanted: u64,
    blocks: VecDeque<Block>,
    budget: usize,
    phase: Phase,
}
impl State {
    fn retained(&self) -> usize {
        self.blocks.iter().map(|block| block.bytes.len()).sum()
    }
    fn discard_consumed(&mut self) {
        while self
            .blocks
            .front()
            .is_some_and(|block| block.end() <= self.wanted)
        {
            self.blocks.pop_front();
        }
    }
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    generation: AtomicU64,
    stopped: AtomicBool,
    size: u64,
}
impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| {
            tracing::error!(%error, "HTTP recording buffer lock poisoned; discarding buffered data");
            let mut state = error.into_inner();
            state.blocks.clear();
            state.phase = Phase::Failed(Arc::new(Error::Runtime(io::Error::other("HTTP buffer lock poisoned"))));
            self.state.clear_poison();
            state
        })
    }
}

#[derive(Clone)]
pub(in crate::playback) struct Control(Weak<Shared>);
impl Control {
    #[cfg(test)]
    pub(super) fn buffered(&self) -> Option<(usize, usize)> {
        self.0.upgrade().map(|shared| {
            let state = shared.state();
            (state.retained(), state.budget)
        })
    }
    pub fn starved(&self) -> bool {
        self.0.upgrade().is_some_and(|shared| {
            let state = shared.state();
            matches!(state.phase, Phase::Retrying) && state.blocks.is_empty()
        })
    }
    pub fn reconnecting(&self) -> bool {
        self.0
            .upgrade()
            .is_some_and(|shared| matches!(shared.state().phase, Phase::Retrying))
    }
    pub fn duration(&self, duration: Duration) {
        let Some(shared) = self.0.upgrade() else {
            return;
        };
        if duration.is_zero() {
            return;
        }
        let bytes = (u128::from(shared.size) * AHEAD_SECONDS * 1_000_000_000)
            .div_ceil(duration.as_nanos())
            .min(MAX_BYTES as u128) as usize;
        let budget = bytes
            .div_ceil(RANGE_BYTES)
            .clamp(1, MAX_BYTES / RANGE_BYTES)
            * RANGE_BYTES;
        let mut state = shared.state();
        if state.budget != budget {
            state.budget = budget;
            // Drain an already filled larger window; changing the estimate
            // must not discard bytes or restart a healthy GET.
            shared.changed.notify_all();
        }
    }
    pub fn interrupt(&self) {
        if let Some(shared) = self.0.upgrade() {
            let mut state = shared.state();
            shared.generation.fetch_add(1, Ordering::AcqRel);
            state.phase = Phase::Superseded;
            shared.changed.notify_all();
        }
    }
    pub fn seek(&self, position: u64) {
        if let Some(shared) = self.0.upgrade() {
            let mut state = shared.state();
            // TS reads confirm their byte position before every block. That
            // confirmation must not cancel a healthy GET or clear a failure.
            if position == state.wanted && !matches!(state.phase, Phase::Superseded) {
                return;
            }
            let reusable = state
                .blocks
                .iter()
                .any(|block| block.start <= position && position < block.end());
            if !reusable || matches!(state.phase, Phase::Failed(_) | Phase::Superseded) {
                shared.generation.fetch_add(1, Ordering::AcqRel);
                if !reusable {
                    state.blocks.clear();
                }
                state.phase = Phase::Ready;
            }
            state.wanted = position;
            state.discard_consumed();
            shared.changed.notify_all();
        }
    }
}

pub(super) struct Buffer {
    source: Verified,
    cancelled: Arc<AtomicBool>,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl Buffer {
    pub fn new(source: Verified, cancelled: Arc<AtomicBool>) -> Self {
        let size = source.size;
        Self {
            source,
            cancelled,
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    wanted: 0,
                    blocks: VecDeque::new(),
                    budget: INITIAL_BYTES,
                    phase: Phase::Ready,
                }),
                changed: Condvar::new(),
                generation: AtomicU64::new(0),
                stopped: AtomicBool::new(false),
                size,
            }),
            worker: None,
        }
    }
    pub fn control(&self) -> Control {
        Control(Arc::downgrade(&self.shared))
    }
    pub fn read(&mut self, position: u64, output: &mut [u8]) -> io::Result<usize> {
        if self.worker.is_none() {
            let source = self.source.clone();
            let shared = self.shared.clone();
            let cancelled = self.cancelled.clone();
            self.worker = Some(
                thread::Builder::new()
                    .name("recording-prefetch".into())
                    .spawn(move || run(source, shared, cancelled))?,
            );
        }
        let mut state = self.shared.state();
        let generation = self.shared.generation.load(Ordering::Acquire);
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(io::Error::other(Error::Cancelled));
            }
            if self.shared.generation.load(Ordering::Acquire) != generation
                || matches!(state.phase, Phase::Superseded)
            {
                return Err(io::Error::other(Error::Cancelled));
            }
            if let Phase::Failed(error) = &state.phase {
                return Err(io::Error::other(error.clone()));
            }
            if let Some(block) = state
                .blocks
                .iter()
                .find(|block| block.start <= position && position < block.end())
            {
                let offset = (position - block.start) as usize;
                let count = output.len().min(block.bytes.len() - offset);
                output[..count].copy_from_slice(&block.bytes[offset..offset + count]);
                state.wanted = position + count as u64;
                state.discard_consumed();
                self.shared.changed.notify_all();
                return Ok(count);
            }
            state = self
                .shared
                .changed
                .wait_timeout(state, CANCEL_POLL)
                .map_err(|_| io::Error::other("HTTP buffer wait lock poisoned"))?
                .0;
        }
    }
}
impl Drop for Buffer {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Release);
        self.shared.changed.notify_all();
        if let Some(worker) = self.worker.take()
            && let Err(error) = worker.join()
        {
            tracing::error!(?error, "Recording prefetch worker panicked during shutdown");
        }
    }
}

fn run(source: Verified, shared: Arc<Shared>, cancelled: Arc<AtomicBool>) {
    let mut connection = None;
    let mut connection_generation = 0;
    loop {
        let mut state = shared.state();
        if shared.stopped.load(Ordering::Acquire) {
            return;
        }
        let generation = shared.generation.load(Ordering::Acquire);
        if generation != connection_generation || cancelled.load(Ordering::Acquire) {
            connection = None;
            connection_generation = generation;
        }
        let end = state.blocks.back().map_or(state.wanted, Block::end);
        let reserve = RANGE_BYTES.min(source.size.saturating_sub(end) as usize);
        if end >= source.size {
            connection = None;
        }
        if cancelled.load(Ordering::Acquire)
            || matches!(state.phase, Phase::Superseded | Phase::Failed(_))
            || end >= source.size
            || state.retained() + reserve > state.budget
        {
            match shared.changed.wait_timeout(state, CANCEL_POLL) {
                Ok(_) => continue,
                Err(error) => {
                    tracing::error!(%error, "Could not wait for HTTP recording buffer capacity");
                    return;
                }
            }
        }
        drop(state);
        let result = stream::next(
            &source,
            end,
            connection.take(),
            &|| {
                shared.stopped.load(Ordering::Acquire)
                    || cancelled.load(Ordering::Acquire)
                    || shared.generation.load(Ordering::Acquire) != generation
            },
            &|_| {
                let mut state = shared.state();
                if shared.generation.load(Ordering::Acquire) == generation {
                    state.phase = Phase::Retrying;
                    shared.changed.notify_all();
                }
            },
        );
        state = shared.state();
        if shared.generation.load(Ordering::Acquire) != generation {
            continue;
        }
        match result {
            Ok((bytes, active)) => {
                // Capacity was reserved before polling the response. A smaller
                // duration estimate only stops subsequent reads until drained.
                debug_assert!(state.retained() + bytes.len() <= MAX_BYTES);
                state.blocks.push_back(Block { start: end, bytes });
                connection = Some(active);
                state.phase = Phase::Ready;
            }
            // Cancellation is expected during seek, suspension and shutdown.
            Err(Error::Cancelled) => {}
            Err(error) => state.phase = Phase::Failed(Arc::new(error)),
        }
        shared.changed.notify_all();
    }
}
