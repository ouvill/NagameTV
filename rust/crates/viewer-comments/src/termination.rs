//! Server termination signals are distinct from transport loss. Preserve the
//! original code/reason while deciding whether another connection is useful.
use crate::retry::Failure;
use serde::Deserialize;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, thiserror::Error)]
#[serde(from = "String")]
pub enum Termination {
    #[error("NX-Jikkyo thread ended (END_PROGRAM)")]
    EndProgram,
    #[error("NX-Jikkyo is temporarily unavailable (SERVICE_TEMPORARILY_UNAVAILABLE)")]
    ServiceUnavailable,
    #[error("NX-Jikkyo disconnected: {0}")]
    OtherNotice(String),
    #[error("NX-Jikkyo WebSocket closed ({code}): {reason}")]
    Close { code: CloseCode, reason: String },
    #[error("NX-Jikkyo WebSocket closed without a status")]
    NoStatus,
}

impl From<String> for Termination {
    fn from(reason: String) -> Self {
        match reason.as_str() {
            "END_PROGRAM" => Self::EndProgram,
            "SERVICE_TEMPORARILY_UNAVAILABLE" => Self::ServiceUnavailable,
            _ => Self::OtherNotice(reason),
        }
    }
}

impl Termination {
    pub(crate) fn from_close(frame: Option<CloseFrame>) -> Self {
        match frame {
            Some(frame) => Self::Close {
                code: frame.code,
                reason: frame.reason.to_string(),
            },
            None => Self::NoStatus,
        }
    }
    pub(crate) fn graceful(&self) -> bool {
        match self {
            Self::EndProgram | Self::NoStatus => true,
            Self::Close { code, .. } => *code == CloseCode::Normal,
            Self::ServiceUnavailable | Self::OtherNotice(_) => false,
        }
    }
    pub(crate) fn failure(&self) -> Failure {
        match self {
            Self::ServiceUnavailable => Failure::Unavailable,
            Self::Close { code, .. } => match code {
                CloseCode::Policy | CloseCode::Unsupported | CloseCode::Invalid => {
                    Failure::Permanent
                }
                CloseCode::Error | CloseCode::Restart => Failure::Unavailable,
                CloseCode::Again => Failure::RateLimited,
                // NX also uses 1002 for a missing/upcoming thread. Do not
                // permanently stop that channel, or infer policy from free text.
                _ => Failure::Temporary,
            },
            Self::EndProgram | Self::OtherNotice(_) | Self::NoStatus => Failure::Temporary,
        }
    }
}
