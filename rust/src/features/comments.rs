//! Playback-independent commentary selection and bounded history.
pub mod activity;
mod mapping;
use crate::{channels::Channel, services::Network};
use std::collections::VecDeque;
use viewer_comments::{
    Comment,
    connection::{Endpoints, State},
    controller::{Controller, Status},
};

const HISTORY_LIMIT: usize = 200;

/// Borrow the failure from its connection generation; presentation owns no error copy.
#[derive(Debug)]
pub enum PresentationStatus<'a> {
    Disabled,
    Unavailable,
    Connecting,
    Receiving,
    Failed(&'a viewer_comments::connection::Error),
    Retrying(Option<&'a viewer_comments::connection::Error>),
}

#[derive(Default)]
pub struct Comments {
    controller: Controller,
    target: Option<(u64, u16)>,
    history: VecDeque<Comment>,
    received: u64,
    pub dirty: bool,
}

fn jikkyo(channel: &Channel) -> Option<u16> {
    let service = channel.broadcast?;
    mapping::resolve(service.network_id, service.service_id)
}

impl Comments {
    pub fn configure(&mut self, enabled: bool, channel: Option<&Channel>) {
        let target = enabled
            .then_some(channel)
            .flatten()
            .and_then(|channel| jikkyo(channel).map(|id| (channel.id, id)));
        if self.target == target {
            return;
        }
        self.target = target;
        self.history = VecDeque::new();
        self.dirty = true;
        self.controller.configure(target.map(|(_, id)| Endpoints {
            threads: format!("https://nx-jikkyo.tsukumijima.net/api/v1/channels/jk{id}/threads"),
            comments: format!("wss://nx-jikkyo.tsukumijima.net/api/v1/channels/jk{id}/ws/comment"),
        }));
    }

    pub fn poll(
        &mut self,
        network: &Network,
        live: impl FnMut(&Comment),
    ) -> Result<(), viewer_comments::controller::Error> {
        let comments = network.poll_comments(&mut self.controller)?;
        self.ingest(comments, live);
        Ok(())
    }

    fn ingest(&mut self, comments: Vec<Comment>, mut live: impl FnMut(&Comment)) {
        for comment in &comments {
            if comment.phase == viewer_comments::Phase::Live {
                live(comment);
            }
        }
        self.append(comments);
    }

    fn append(&mut self, comments: impl IntoIterator<Item = Comment>) {
        for comment in comments {
            if self.history.len() == HISTORY_LIMIT {
                self.history.pop_front();
            }
            self.received += 1;
            self.history.push_back(comment);
            self.dirty = true;
        }
    }

    pub fn storage(&self) -> (usize, usize) {
        (
            self.history.len(),
            self.history.iter().map(|comment| comment.text.len()).sum(),
        )
    }
    pub fn status(&self, enabled: bool) -> PresentationStatus<'_> {
        if !enabled {
            return PresentationStatus::Disabled;
        }
        if self.target.is_none() {
            return PresentationStatus::Unavailable;
        }
        match self.controller.status() {
            Status::Disabled | Status::Switching | Status::Connection(State::Connecting) => {
                PresentationStatus::Connecting
            }
            Status::Connection(State::Receiving) => PresentationStatus::Receiving,
            Status::Connection(State::Failed(error)) => PresentationStatus::Failed(error),
            Status::Retrying(State::Failed(error)) => PresentationStatus::Retrying(Some(error)),
            Status::Connection(State::Ended) | Status::Retrying(_) => {
                PresentationStatus::Retrying(None)
            }
        }
    }

    pub fn json(&self) -> Result<String, serde_json::Error> {
        #[derive(serde::Serialize)]
        struct Row<'a> {
            id: String,
            time: String,
            text: &'a str,
            source: viewer_comments::Origin,
        }
        struct Rows<'a>(&'a VecDeque<Comment>, u64);
        impl serde::Serialize for Rows<'_> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                // Stream borrowed history in arrival order. Each formatted time
                // lives for one row; no second array of all visible rows is retained.
                serializer.collect_seq(self.0.iter().enumerate().map(|(index, comment)| Row {
                    id: (self.1 - self.0.len() as u64 + index as u64).to_string(),
                    time: comment.japan_time(),
                    text: &comment.text,
                    source: comment.origin,
                }))
            }
        }
        serde_json::to_string(&Rows(&self.history, self.received))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(
        id: u64,
        band: &str,
        name: &str,
        service: Option<u16>,
    ) -> Result<Channel, Box<dyn std::error::Error>> {
        let bytes = serde_json::to_vec(&serde_json::json!([{
            "id": id, "type": 1, "name": name, "channel": {"type": band},
            "networkId": 4, "serviceId": service
        }]))?;
        crate::channels::parse(&bytes)?
            .pop()
            .ok_or_else(|| "missing fixture channel".into())
    }

    #[test]
    fn mapping_uses_broadcast_metadata_not_endpoint_arithmetic()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            jikkyo(&channel(u64::MAX, "BS", "NHK", Some(102))?),
            Some(101)
        );
        assert_eq!(jikkyo(&channel(400101, "BS", "NHK", None)?), None);
        assert_eq!(jikkyo(&channel(1, "GR", "ＮＨＫ総合１", Some(999))?), None);
        assert_eq!(jikkyo(&channel(1, "GR", "未対応", Some(101))?), Some(101));
        assert_eq!(
            jikkyo(&channel(1, "OTHER", "ＮＨＫ総合", Some(101))?),
            Some(101)
        );
        assert_eq!(
            jikkyo(&channel(999, "OTHER", "Other", Some(102))?),
            Some(101)
        );
        assert_eq!(jikkyo(&channel(101, "OTHER", "Other", None)?), None);
        Ok(())
    }

    #[test]
    fn only_new_comments_are_projected_to_motion_and_both_phases_enter_history() {
        let mut comments = Comments::default();
        let mut live = Vec::new();
        comments.ingest(
            vec![
                Comment {
                    text: "old".into(),
                    origin: viewer_comments::Origin::Nx,
                    phase: viewer_comments::Phase::History,
                    unix_seconds: 0,
                    style: viewer_comments::Style::default(),
                },
                Comment {
                    text: "new".into(),
                    origin: viewer_comments::Origin::Nx,
                    phase: viewer_comments::Phase::Live,
                    unix_seconds: 1,
                    style: viewer_comments::Style {
                        position: viewer_comments::Position::Top,
                        color: 0xff0000,
                    },
                },
            ],
            |comment| live.push((comment.text.to_string(), comment.style)),
        );
        assert_eq!(
            live,
            [(
                "new".to_owned(),
                viewer_comments::Style {
                    position: viewer_comments::Position::Top,
                    color: 0xff0000
                }
            )]
        );
        assert_eq!(comments.history.len(), 2);
        live.clear();
        comments.ingest(Vec::new(), |comment| {
            live.push((comment.text.to_string(), comment.style))
        });
        assert!(live.is_empty());
        assert_eq!(comments.history.len(), 2);
    }

    #[test]
    fn history_is_bounded_and_released_on_switch_and_disable()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut comments = Comments::default();
        assert!(matches!(
            comments.status(false),
            PresentationStatus::Disabled
        ));
        assert!(matches!(
            comments.status(true),
            PresentationStatus::Unavailable
        ));
        let first = channel(1, "BS", "NHK", Some(101))?;
        let second = channel(2, "BS", "NHK", Some(102))?;
        comments.configure(true, Some(&first));
        assert!(matches!(
            comments.status(true),
            PresentationStatus::Connecting
        ));
        comments.append((0..1000).map(|i| Comment {
            text: format!("<b>{i}</b>\n日本語").into_boxed_str(),
            origin: viewer_comments::Origin::Nx,
            phase: viewer_comments::Phase::Live,
            unix_seconds: i,
            style: viewer_comments::Style::default(),
        }));
        let rows: Vec<serde_json::Value> = serde_json::from_str(&comments.json()?)?;
        assert_eq!(rows.len(), HISTORY_LIMIT);
        for (row, second) in rows.iter().zip(800..1000) {
            assert_eq!(row["text"], format!("<b>{second}</b>\n日本語"));
            assert_eq!(
                row["time"],
                format!("09:{:02}:{:02}", second / 60, second % 60)
            );
            assert_eq!(row["source"], "NX");
            assert_eq!(row["id"], second.to_string());
        }
        comments.dirty = false;
        comments.configure(true, Some(&first));
        assert!(!comments.dirty);
        assert_eq!(comments.history.len(), HISTORY_LIMIT);
        // Distinct broadcasts sharing jk101 must still clear the visible history.
        comments.configure(true, Some(&second));
        assert!(comments.dirty);
        assert_eq!(comments.history.capacity(), 0);
        comments.configure(false, Some(&second));
        assert_eq!(comments.json()?, "[]");
        assert!(comments.target.is_none());
        assert!(comments.controller.is_stopped());
        assert!(matches!(
            comments.status(false),
            PresentationStatus::Disabled
        ));
        Ok(())
    }
}
