//! Hardware-free tests of the actual appsrc reader, with controlled arrivals.
use super::*;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};

const DEADLINE: Duration = Duration::from_secs(5);
// Only negative observations wait in real time; handshakes position the reader
// at its blocking wait before either measuring idleness or sending a change.
const QUIET_WINDOW: Duration = Duration::from_millis(50);

struct ReadTask {
    feeder: Feeder,
    finished: Receiver<Result<Option<gst::Buffer>, Error>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl ReadTask {
    fn start(feeder: &Feeder) -> Self {
        let (sender, finished) = mpsc::channel();
        let reading = feeder.clone();
        let generation = reading.feedback.generation.load(Ordering::Acquire);
        let thread = std::thread::spawn(move || {
            let result = reading.next(generation, &Mutex::new(None));
            if sender.send(result).is_err() {
                // The test can unwind before cancellation has finished.
            }
        });
        Self {
            feeder: feeder.clone(),
            finished,
            thread: Some(thread),
        }
    }
    fn result(&self) -> Result<Option<gst::Buffer>, Error> {
        self.finished
            .recv_timeout(DEADLINE)
            .expect("reader did not finish")
    }
}
impl Drop for ReadTask {
    fn drop(&mut self) {
        self.feeder.suspend(true);
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("TS wait test reader panicked");
        }
    }
}

struct LiveInput {
    feeder: Feeder,
    store: Arc<Mutex<Store>>,
}

fn live() -> Result<LiveInput, Box<dyn std::error::Error>> {
    gst::init()?;
    let store = Arc::new(Mutex::new(Store::new(Retention::Off, 1, false)?));
    let reader = Reader {
        source: ReaderSource::Live(store.clone()),
        shared: Shared::Live(store.clone()),
        offset: 0,
        framing: Framing::transport(),
        service: 1,
        filter: tsreadex::Filter::new(1)?,
        clock: Index::new(1, false),
        bootstrap: Vec::new(),
        time_ns: 0,
        epoch: 0,
        pending: VecDeque::new(),
        pending_seek: None,
    };
    Ok(LiveInput {
        feeder: Feeder::new(reader)?,
        store,
    })
}

fn null_block() -> Vec<u8> {
    use crate::transport::wire::{Pid, STUFFING_BYTE, SYNC_BYTE};
    let mut packet = [STUFFING_BYTE; TS_PACKET_SIZE];
    const HEADER_SIZE: usize = 4;
    const PAYLOAD_ONLY: u8 = 0x10;
    packet[..HEADER_SIZE].copy_from_slice(&[
        SYNC_BYTE,
        (Pid::NULL.0 >> u8::BITS) as u8,
        Pid::NULL.0 as u8,
        PAYLOAD_ONLY,
    ]);
    packet.repeat(READ_BYTES / TS_PACKET_SIZE)
}

#[test]
fn empty_input_sleeps_until_suspended_or_seeked() -> Result<(), Box<dyn std::error::Error>> {
    for seek in [false, true] {
        let LiveInput { feeder, store } = live()?;
        let waiting = feeder.feedback.activity.watch_waits();
        let task = ReadTask::start(&feeder);
        waiting.recv_timeout(DEADLINE)?;
        assert_eq!(feeder.feedback.read_progress()?.waits, 1);
        store.lock().unwrap().append(&[])?;
        assert_eq!(
            waiting.recv_timeout(QUIET_WINDOW),
            Err(RecvTimeoutError::Timeout)
        );
        assert_eq!(feeder.feedback.read_progress()?.waits, 1);
        if seek {
            feeder.feedback.begin_seek();
        } else {
            feeder.suspend(true);
        }
        assert!(matches!(task.result(), Err(Error::Cancelled)));
    }
    Ok(())
}

#[test]
fn filtered_input_advances_then_blocks_until_playable_data_arrives()
-> Result<(), Box<dyn std::error::Error>> {
    let LiveInput { feeder, store } = live()?;
    let waiting = feeder.feedback.activity.watch_waits();
    let task = ReadTask::start(&feeder);
    waiting.recv_timeout(DEADLINE)?;
    // Enough discarded data to exercise bounded yields as well as the final
    // empty-input wait, while remaining inside the forward buffer's budget.
    const FILTERED_BLOCKS: usize = 24;
    let bytes = null_block().repeat(FILTERED_BLOCKS);
    store.lock().unwrap().append(&bytes)?;
    waiting.recv_timeout(DEADLINE)?;
    assert_eq!(feeder.reader.lock().unwrap().offset, bytes.len() as u64);
    assert_eq!(feeder.feedback.read_progress()?.waits, 2);
    assert_eq!(
        waiting.recv_timeout(QUIET_WINDOW),
        Err(RecvTimeoutError::Timeout)
    );
    let fixture = include_bytes!("../../../../../tests/fixtures/recording.ts");
    store.lock().unwrap().append(&fixture[..READ_BYTES])?;
    assert!(task.result()?.is_some_and(|buffer| buffer.size() > 0));
    Ok(())
}

#[test]
fn end_and_failure_wake_an_empty_input() -> Result<(), Box<dyn std::error::Error>> {
    for failed in [false, true] {
        let LiveInput { feeder, store } = live()?;
        let waiting = feeder.feedback.activity.watch_waits();
        let task = ReadTask::start(&feeder);
        waiting.recv_timeout(DEADLINE)?;
        store.lock().unwrap().finish(if failed {
            Err("receive failed".into())
        } else {
            Ok(())
        });
        match (failed, task.result()) {
            (false, Ok(None)) => {}
            (true, Err(Error::Io(error))) => assert_eq!(error.to_string(), "receive failed"),
            _ => panic!("terminal reception state was not delivered"),
        }
    }
    Ok(())
}

#[test]
fn expired_position_ignores_arrivals_and_waits_for_control()
-> Result<(), Box<dyn std::error::Error>> {
    let LiveInput { feeder, store } = live()?;
    let block = null_block();
    let (byte_limit, _) = Policy::default().budget();
    for _ in 0..byte_limit as usize / READ_BYTES + 2 {
        store.lock().unwrap().append(&block)?;
    }
    assert!(store.lock().unwrap().start() > 0);
    let waiting = feeder.feedback.activity.watch_waits();
    let task = ReadTask::start(&feeder);
    waiting.recv_timeout(DEADLINE)?;
    assert!(feeder.feedback.take_expired());
    store.lock().unwrap().append(&block)?;
    assert_eq!(
        waiting.recv_timeout(QUIET_WINDOW),
        Err(RecvTimeoutError::Timeout)
    );
    assert!(!feeder.feedback.take_expired());
    feeder.feedback.begin_seek();
    assert!(matches!(task.result(), Err(Error::Cancelled)));
    Ok(())
}
