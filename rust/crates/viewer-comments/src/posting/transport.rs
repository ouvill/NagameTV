use super::{
    Error, Outcome,
    protocol::{self, Event},
};
use crate::MAX_MESSAGE_BYTES;
use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};

/// Connect only for an explicit post. The whole attempt, including handshake,
/// control replies and acknowledgement, has one deadline shorter than NX's seat interval.
pub(super) async fn post(
    url: String,
    text: String,
    deadline: Duration,
    echoes: std::sync::mpsc::SyncSender<super::echo::Echo>,
) -> Outcome {
    let mut sent = false;
    let mut acknowledged = false;
    let result = timeout(deadline, async {
        let config = WebSocketConfig::default()
            .read_buffer_size(8 * 1024)
            .write_buffer_size(0)
            .max_write_buffer_size(MAX_MESSAGE_BYTES)
            .max_message_size(Some(MAX_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_MESSAGE_BYTES));
        let (mut socket, _) = connect_async_with_config(url, Some(config), false).await?;
        socket
            .send(Message::Text(
                r#"{"type":"startWatching","data":{"reconnect":false}}"#.into(),
            ))
            .await?;
        let mut clock = None;
        let mut room = None;
        while let Some(message) = socket.next().await {
            match message? {
                Message::Text(value) => {
                    match serde_json::from_str::<Event>(&value)? {
                        Event::Time { data } => {
                            clock = Some((protocol::timestamp(&data.current)?, Instant::now()))
                        }
                        Event::Room { data } => {
                            room = Some((protocol::timestamp(&data.base)?, data))
                        }
                        Event::Ping => {
                            socket
                                .send(Message::Text(r#"{"type":"pong"}"#.into()))
                                .await?;
                            socket
                                .send(Message::Text(r#"{"type":"keepSeat"}"#.into()))
                                .await?;
                        }
                        Event::Result { data } if sent => {
                            let chat = data.chat;
                            if chat.content != text {
                                return Err(Error::Session);
                            }
                            if chat.restricted {
                                return Err(Error::Rejected("RESTRICTED".into()));
                            }
                            acknowledged = true;
                            // Once acknowledged, closing errors cannot turn this into a failed post.
                            let _ = timeout(Duration::from_millis(200), socket.close(None)).await;
                            return Ok(());
                        }
                        Event::Error { data } => return Err(Error::Rejected(data.message)),
                        Event::Disconnect { data } => return Err(Error::Rejected(data.reason)),
                        _ => {}
                    }
                    if !sent
                        && let (Some((current, observed)), Some((base, room))) =
                            (clock, room.as_ref())
                    {
                        let current = current + observed.elapsed().as_millis() as i128;
                        let body = protocol::request(&text, current, *base)?;
                        if let Some(identity) = room.identity(protocol::vpos(current, *base)?) {
                            let _ = echoes.try_send(super::echo::Echo {
                                identity,
                                text: text.as_str().into(),
                                created: Instant::now(),
                            });
                        }
                        // Mark before the await: partial writes and lost acknowledgements are ambiguous.
                        sent = true;
                        socket.send(Message::Text(body.into())).await?;
                    }
                }
                Message::Ping(_) => socket.flush().await?,
                Message::Close(_) => return Err(Error::Closed),
                _ => {}
            }
        }
        Err(Error::Closed)
    })
    .await;
    if acknowledged {
        return Outcome::Sent;
    }
    match result {
        Ok(Ok(())) => Outcome::Sent,
        other => {
            let error = match other {
                Ok(Err(error)) => error,
                _ => Error::Timeout,
            };
            // NX may report INVALID_MESSAGE even after its DB transaction committed.
            // Treat every post-write failure conservatively, except explicit restriction.
            if sent
                && !matches!(&error, Error::Rejected(code) if code == "RESTRICTED" || code == "NOT_ON_AIR")
            {
                Outcome::Unknown(error)
            } else {
                Outcome::Failed(error)
            }
        }
    }
}
