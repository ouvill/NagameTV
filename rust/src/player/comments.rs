use super::ffi;
use crate::comments::CommentEvent;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use std::{pin::Pin, sync::atomic::Ordering};

impl ffi::Player {
    pub(super) fn restart_comments(mut self: Pin<&mut Self>) {
        let current_id = self.as_ref().service_id().to_string().parse::<u64>().ok();
        let jikkyo = current_id.and_then(|id| {
            let index = self
                .as_ref()
                .rust()
                .service_ids
                .iter()
                .position(|item| *item == id)?;
            self.as_ref().rust().jikkyo_ids.get(index)?.clone()
        });
        if jikkyo.is_some()
            && self.as_ref().rust().active_jikkyo_id == jikkyo
            && self
                .as_ref()
                .rust()
                .comment_task
                .as_ref()
                .is_some_and(|task| !task.0.is_finished())
        {
            return;
        }
        let generation = self
            .as_ref()
            .rust()
            .comment_generation
            .fetch_add(1, Ordering::AcqRel)
            + 1;
        self.as_mut().rust_mut().comment_task = None;
        while self.as_ref().rust().comment_events_rx.try_recv().is_ok() {}
        self.as_mut().rust_mut().active_jikkyo_id = jikkyo.clone();
        self.as_mut().rust_mut().comments.clear();
        self.as_mut().set_comment_times(QStringList::default());
        self.as_mut().set_comment_texts(QStringList::default());
        self.as_mut().set_comment_sources(QStringList::default());
        let Some(channel_id) = jikkyo else {
            self.as_mut()
                .set_comment_status(QString::from("Comments are unavailable for this channel"));
            return;
        };
        let network = self
            .as_ref()
            .rust()
            .network
            .as_ref()
            .map(|network| (network.handle(), network.client()));
        let Some((handle, client)) = network else {
            return;
        };
        self.as_mut()
            .set_comment_status(QString::from("Connecting to comments…"));
        let current_generation = self.as_ref().rust().comment_generation.clone();
        let events = self.as_ref().rust().comment_events_tx.clone();
        self.as_mut().rust_mut().comment_task = Some(crate::network::NetworkTask(handle.spawn(
            crate::comments::receive(client, channel_id, generation, current_generation, events),
        )));
    }

    pub(super) fn poll_comments(mut self: Pin<&mut Self>) {
        let comment_events = self
            .as_ref()
            .rust()
            .comment_events_rx
            .try_iter()
            .take(64)
            .collect::<Vec<_>>();
        let mut comments_changed = false;
        let mut received_texts = Vec::new();
        for (generation, event) in comment_events {
            if generation
                != self
                    .as_ref()
                    .rust()
                    .comment_generation
                    .load(Ordering::Acquire)
            {
                continue;
            }
            match event {
                CommentEvent::Status(status) => {
                    self.as_mut().set_comment_status(QString::from(status))
                }
                CommentEvent::Comment {
                    time,
                    text,
                    source,
                    initial,
                } => {
                    if !initial {
                        received_texts.push(text.clone());
                    }
                    let mut rust = self.as_mut().rust_mut();
                    rust.comments.push(CommentEntry { time, text, source });
                    comments_changed = true;
                }
            }
        }
        if comments_changed {
            let (times, texts, sources) = {
                let player = self.as_ref();
                let rust = player.rust();
                (
                    rust.comments
                        .iter()
                        .map(|item| QString::from(&item.time))
                        .collect(),
                    rust.comments
                        .iter()
                        .map(|item| QString::from(&item.text))
                        .collect(),
                    rust.comments
                        .iter()
                        .map(|item| QString::from(&item.source))
                        .collect(),
                )
            };
            self.as_mut().set_comment_times(times);
            self.as_mut().set_comment_texts(texts);
            self.as_mut().set_comment_sources(sources);
        }
        for text in received_texts {
            self.as_mut().comment_received(QString::from(text));
        }
    }
}

pub(super) struct CommentEntry {
    pub time: String,
    pub text: String,
    pub source: String,
}

/// Owns the retention limit, so no caller can append an unbounded history.
#[derive(Default)]
pub(super) struct CommentHistory(std::collections::VecDeque<CommentEntry>);

impl CommentHistory {
    const LIMIT: usize = 200;

    fn push(&mut self, entry: CommentEntry) {
        if self.0.len() == Self::LIMIT {
            self.0.pop_front();
        }
        self.0.push_back(entry);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn iter(&self) -> impl Iterator<Item = &CommentEntry> {
        self.0.iter()
    }
    fn clear(&mut self) {
        self.0.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{CommentEntry, CommentHistory};

    #[test]
    fn history_evicts_oldest_and_preserves_order() {
        let mut history = CommentHistory::default();
        for index in 0..1000 {
            history.push(CommentEntry {
                time: String::new(),
                text: index.to_string(),
                source: String::new(),
            });
        }
        assert_eq!(history.len(), 200);
        assert_eq!(
            history.iter().next().map(|entry| entry.text.as_str()),
            Some("800")
        );
        assert_eq!(
            history.iter().last().map(|entry| entry.text.as_str()),
            Some("999")
        );
        history.clear();
        assert_eq!(history.len(), 0);
    }
}
