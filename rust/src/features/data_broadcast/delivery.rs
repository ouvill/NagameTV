//! Versioned session control wraps the unchanged web-bml message schema.
//! Only connection tasks serialize or await the browser; the receive actor never does.
use super::{OUTPUT_WAIT, Update, metadata};
use futures_util::{Sink, SinkExt};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;

pub(super) enum Delivery {
    Snapshot {
        epoch: u64,
        updates: Vec<Arc<Update>>,
    },
    Update {
        epoch: u64,
        update: Arc<Update>,
    },
    Fault(&'static str),
}

impl Delivery {
    pub fn resource_bytes(&self) -> usize {
        match self {
            Self::Snapshot { updates, .. } => updates.iter().map(|u| u.resource_bytes()).sum(),
            Self::Update { update, .. } => update.resource_bytes(),
            Self::Fault(_) => 0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("WebSocket: {0}")]
    Socket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("browser write timed out: {0}")]
    Timeout(#[from] tokio::time::error::Elapsed),
    #[error("serializing browser message: {0}")]
    Serialize(#[from] serde_json::Error),
}

async fn frame<S>(socket: &mut S, message: impl Serialize) -> Result<(), Error>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let text = serde_json::to_string(&message)?;
    tokio::time::timeout(OUTPUT_WAIT, socket.send(Message::Text(text.into()))).await??;
    Ok(())
}

#[derive(Serialize)]
struct Envelope<T> {
    #[serde(rename = "type")]
    kind: &'static str,
    epoch: String,
    message: T,
}

async fn update<S>(socket: &mut S, epoch: u64, update: &Update) -> Result<(), Error>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    if let Update::Broadcast(update) = update {
        return match viewer_web_bml::prepare(update) {
            Ok(message) => {
                frame(
                    socket,
                    Envelope {
                        kind: "update",
                        epoch: epoch.to_string(),
                        message,
                    },
                )
                .await
            }
            Err(error) => {
                tracing::warn!(
                    operation = "convert data broadcast update for web-bml",
                    error = &error as &dyn std::error::Error
                );
                // An unsupported upstream representation must not stop TS reception.
                Ok(())
            }
        };
    }
    let message = match update {
        Update::Broadcast(_) => unreachable!("broadcast updates handled above"),
        Update::Metadata(metadata::Update::Program(program)) => viewer_web_bml::program(program),
        Update::Metadata(metadata::Update::Time(time)) => viewer_web_bml::current_time(*time),
        Update::Metadata(metadata::Update::Clock { base, extension }) => {
            viewer_web_bml::pcr(*base, *extension)
        }
    };
    frame(
        socket,
        Envelope {
            kind: "update",
            epoch: epoch.to_string(),
            message,
        },
    )
    .await
}
pub(super) async fn send<S>(socket: &mut S, delivery: Delivery) -> Result<(), Error>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    match delivery {
        Delivery::Snapshot { epoch, updates } => {
            frame(
                socket,
                json!({"type": "begin", "version": 1, "epoch": epoch.to_string()}),
            )
            .await?;
            for item in updates {
                update(socket, epoch, &item).await?;
            }
            frame(socket, json!({"type": "ready", "epoch": epoch.to_string()})).await
        }
        Delivery::Update {
            epoch,
            update: item,
        } => update(socket, epoch, &item).await,
        Delivery::Fault(reason) => frame(socket, json!({"type": "fault", "reason": reason})).await,
    }
}
