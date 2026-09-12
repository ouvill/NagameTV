use crate::{Comment, CommentIdentity, Origin, Phase};
use std::{
    collections::VecDeque,
    sync::mpsc,
    time::{Duration, Instant},
};

const ECHO_LIFETIME: Duration = Duration::from_secs(60);
const MAX_ECHOES: usize = 64;

pub(super) struct Echo {
    pub identity: CommentIdentity,
    pub text: Box<str>,
    pub created: Instant,
}

#[derive(Default)]
pub(super) struct Echoes {
    incoming: Option<mpsc::Receiver<Echo>>,
    pending: VecDeque<Echo>,
}

impl Echoes {
    pub fn collect(&mut self, now: Instant) {
        if let Some(echo) = self.incoming.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.incoming = None;
            if self.pending.len() == MAX_ECHOES {
                self.pending.pop_front();
            }
            self.pending.push_back(echo);
        }
        self.pending
            .retain(|echo| now.saturating_duration_since(echo.created) < ECHO_LIFETIME);
    }

    pub fn start(&mut self, now: Instant) -> mpsc::SyncSender<Echo> {
        self.collect(now);
        let (tx, rx) = mpsc::sync_channel(1);
        self.incoming = Some(rx);
        tx
    }

    pub fn take_match(&mut self, comment: &Comment, now: Instant) -> bool {
        // Published before the socket write, so even an echo preceding the ACK
        // is recognizable. Consume exactly one match, never guess by text alone.
        self.collect(now);
        if comment.phase != Phase::Live || comment.origin != Origin::Nx {
            return false;
        }
        let Some(identity) = &comment.identity else {
            return false;
        };
        let Some(index) = self
            .pending
            .iter()
            .position(|echo| echo.identity == *identity && echo.text == comment.text)
        else {
            return false;
        };
        self.pending.remove(index);
        true
    }
}
