use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc::SyncSender};
use std::time::Duration;
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};

pub const QUEUE_CAPACITY: usize = 256;
const MAX_COMMENT_BYTES: usize = 4096;

const NX_JIKKYO: &str = "https://nx-jikkyo.tsukumijima.net";

#[derive(Debug)]
pub enum CommentEvent {
    Status(String),
    Comment {
        time: String,
        text: String,
        source: String,
        initial: bool,
    },
}

#[derive(Deserialize)]
struct Thread {
    id: u64,
    status: String,
}

pub fn jikkyo_id(channel_type: &str, service_id: u16, name: &str) -> Option<String> {
    if channel_type == "GR" {
        let id = if name.contains("ＮＨＫ総合") {
            1
        } else if name.contains("Ｅテレ") {
            2
        } else if name.contains("読売テレビ") {
            4
        } else if name.contains("ＡＢＣテレビ") {
            5
        } else if name.contains("ＭＢＳ") {
            6
        } else if name.contains("関西テレビ") {
            8
        } else if name.contains("ＫＢＳ京都") {
            14
        } else {
            return None;
        };
        return Some(format!("jk{id}"));
    }
    let id = if service_id == 102 { 101 } else { service_id };
    Some(format!("jk{id}"))
}

pub async fn receive(
    client: reqwest::Client,
    channel_id: String,
    generation: u64,
    current_generation: Arc<AtomicU64>,
    events: SyncSender<(u64, CommentEvent)>,
) {
    let events = EventSender {
        generation,
        current: current_generation.clone(),
        queue: events,
    };
    let result = receive_inner(
        &client,
        &channel_id,
        generation,
        &current_generation,
        &events,
    )
    .await;
    if current_generation.load(Ordering::Acquire) == generation {
        let status = result.map_or_else(
            |error| format!("Comment connection error: {error}"),
            |_| "Connection ended".to_owned(),
        );
        events.send(CommentEvent::Status(status));
    }
}

// Never block the single network worker when the UI falls behind. Drop incoming
// messages under overload; history and on-screen comments have separate limits.
struct EventSender {
    generation: u64,
    current: Arc<AtomicU64>,
    queue: SyncSender<(u64, CommentEvent)>,
}

impl EventSender {
    fn send(&self, event: CommentEvent) {
        if self.current.load(Ordering::Acquire) == self.generation {
            let _ = self.queue.try_send((self.generation, event));
        }
    }
}

async fn receive_inner(
    client: &reqwest::Client,
    channel_id: &str,
    generation: u64,
    current_generation: &AtomicU64,
    events: &EventSender,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let threads = client
        .get(format!("{NX_JIKKYO}/api/v1/channels/{channel_id}/threads"))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Thread>>()
        .await?;
    let thread = threads
        .iter()
        .find(|thread| thread.status == "ACTIVE")
        .ok_or("No active comment thread")?;
    if current_generation.load(Ordering::Acquire) != generation {
        return Ok(());
    }

    let url = format!("wss://nx-jikkyo.tsukumijima.net/api/v1/channels/{channel_id}/ws/comment");
    let (mut socket, _) = tokio::time::timeout(
        Duration::from_secs(10),
        connect_async_with_config(
            url,
            Some(
                WebSocketConfig::default()
                    .max_message_size(Some(64 * 1024))
                    .max_frame_size(Some(64 * 1024)),
            ),
            false,
        ),
    )
    .await??;
    let request = json!([
        {"ping":{"content":"rs:0"}},
        {"ping":{"content":"ps:0"}},
        {"thread":{"thread":thread.id.to_string(),"version":"20061206","user_id":"guest","res_from":-100,"with_global":1,"scores":1,"nicoru":0}},
        {"ping":{"content":"pf:0"}},
        {"ping":{"content":"rf:0"}}
    ]);
    tokio::time::timeout(
        Duration::from_secs(10),
        socket.send(Message::Text(request.to_string().into())),
    )
    .await??;
    events.send(CommentEvent::Status("Receiving comments".to_owned()));
    let mut receiving_initial_comments = true;

    loop {
        if current_generation.load(Ordering::Acquire) != generation {
            let _ = tokio::time::timeout(Duration::from_secs(2), socket.close(None)).await;
            return Ok(());
        }
        let message = match tokio::time::timeout(Duration::from_secs(1), socket.next()).await {
            Ok(Some(message)) => message,
            Ok(None) => return Ok(()),
            Err(_) => continue,
        };
        let Message::Text(text) = message? else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if value.pointer("/ping/content").and_then(Value::as_str) == Some("rf:0") {
            receiving_initial_comments = false;
            continue;
        }
        let Some(chat) = value.get("chat") else {
            continue;
        };
        let Some(content) = chat.get("content").and_then(Value::as_str) else {
            continue;
        };
        if content.is_empty() || content.len() > MAX_COMMENT_BYTES {
            continue;
        }
        let seconds = (chat.get("date").and_then(Value::as_u64).unwrap_or(0) + 9 * 3_600) % 86_400;
        let time = format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        );
        let user = chat.get("user_id").and_then(Value::as_str).unwrap_or("");
        let source = if user.starts_with("nicolive:") || user.starts_with("rekari:") {
            "ニコ実"
        } else {
            "NX"
        };
        events.send(CommentEvent::Comment {
            time,
            text: content.to_owned(),
            source: source.to_owned(),
            initial: receiving_initial_comments,
        });
    }
}

#[cfg(test)]
mod tests {
    // In tests, unwrap/expect assert successful setup or an expected result.
    // Failures intentionally fail the test; they are not assumed impossible IO.
    use super::{CommentEvent, jikkyo_id, receive};
    use std::sync::atomic::AtomicU64;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    #[test]
    fn queue_is_bounded_and_rejects_obsolete_connections() {
        let (tx, rx) = mpsc::sync_channel(2);
        let current = Arc::new(AtomicU64::new(1));
        let sender = super::EventSender {
            generation: 1,
            current: current.clone(),
            queue: tx,
        };
        for _ in 0..1000 {
            sender.send(CommentEvent::Status("test".into()));
        }
        assert_eq!(rx.try_iter().count(), 2);
        current.store(2, std::sync::atomic::Ordering::Release);
        sender.send(CommentEvent::Status("stale".into()));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn maps_kansai_terrestrial_affiliates() {
        assert_eq!(jikkyo_id("GR", 2064, "ＭＢＳ"), Some("jk6".to_owned()));
        assert_eq!(
            jikkyo_id("GR", 2072, "ＡＢＣテレビ１"),
            Some("jk5".to_owned())
        );
        assert_eq!(
            jikkyo_id("GR", 2088, "読売テレビ１"),
            Some("jk4".to_owned())
        );
    }

    #[test]
    fn maps_satellite_service_ids() {
        assert_eq!(
            jikkyo_id("BS", 101, "ＮＨＫ　ＢＳ"),
            Some("jk101".to_owned())
        );
        assert_eq!(jikkyo_id("BS", 200, "ＢＳ１０"), Some("jk200".to_owned()));
    }

    #[test]
    #[ignore = "requires the public NX-Jikkyo service"]
    fn receives_a_live_comment() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let generation = Arc::new(AtomicU64::new(1));
        let (sender, receiver) = mpsc::sync_channel(super::QUEUE_CAPACITY);
        runtime.spawn(receive(client, "jk1".to_owned(), 1, generation, sender));
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut received = false;
        while std::time::Instant::now() < deadline {
            if matches!(
                receiver.recv_timeout(Duration::from_millis(500)),
                Ok((1, CommentEvent::Comment { initial: false, .. }))
            ) {
                received = true;
                break;
            }
        }
        assert!(
            received,
            "NX-Jikkyo did not deliver a real-time comment within 20 seconds"
        );
    }
}
