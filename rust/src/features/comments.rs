//! Playback-independent commentary selection and bounded history.
use crate::{
    channels::{Band, Channel},
    services::Network,
};
use std::collections::VecDeque;
use viewer_comments::{
    Comment,
    connection::{Endpoints, State},
    controller::{Controller, Status},
};

const HISTORY_LIMIT: usize = 200;

#[derive(Default)]
pub struct Comments {
    controller: Controller,
    target: Option<(u64, u16)>,
    history: VecDeque<Comment>,
    pub dirty: bool,
}

fn jikkyo(channel: &Channel) -> Option<u16> {
    match channel.band {
        Band::Terrestrial => [
            ("ＮＨＫ総合", 1),
            ("Ｅテレ", 2),
            ("読売テレビ", 4),
            ("ＡＢＣテレビ", 5),
            ("ＭＢＳ", 6),
            ("関西テレビ", 8),
            ("ＫＢＳ京都", 14),
        ]
        .into_iter()
        .find(|(name, _)| channel.name.contains(name))
        .map(|(_, id)| id),
        Band::Bs | Band::Cs | Band::Sky => channel.broadcast.map(|service| {
            if service.service_id == 102 {
                101
            } else {
                service.service_id
            }
        }),
        Band::Other => None,
    }
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

    pub fn poll(&mut self, network: &Network, live: impl FnMut(&str)) {
        let comments = network.poll_comments(&mut self.controller);
        self.ingest(comments, live);
    }

    fn ingest(&mut self, comments: Vec<Comment>, mut live: impl FnMut(&str)) {
        for comment in &comments {
            if comment.phase == viewer_comments::Phase::Live {
                live(&comment.text);
            }
        }
        self.append(comments);
    }

    fn append(&mut self, comments: impl IntoIterator<Item = Comment>) {
        for comment in comments {
            if self.history.len() == HISTORY_LIMIT {
                self.history.pop_front();
            }
            self.history.push_back(comment);
            self.dirty = true;
        }
    }

    pub fn status(&self, enabled: bool) -> String {
        if !enabled {
            return "無効".into();
        }
        if self.target.is_none() {
            return "実況対象のチャンネルを選んでください".into();
        }
        match self.controller.status() {
            Status::Disabled | Status::Switching | Status::Connection(State::Connecting) => {
                "接続中…".into()
            }
            Status::Connection(State::Receiving) => "実況を受信中".into(),
            Status::Connection(State::Failed(error)) => error.to_string(),
            Status::Retrying(State::Failed(error)) => format!("{error}（再接続待ち）"),
            Status::Connection(State::Ended) | Status::Retrying(_) => "再接続待ち".into(),
        }
    }

    pub fn json(&self) -> Result<String, serde_json::Error> {
        #[derive(serde::Serialize)]
        struct Row<'a> {
            time: String,
            text: &'a str,
            source: viewer_comments::Origin,
        }
        let rows: Vec<_> = self
            .history
            .iter()
            .map(|comment| Row {
                time: comment.japan_time(),
                text: &comment.text,
                source: comment.origin,
            })
            .collect();
        serde_json::to_string(&rows)
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
        assert_eq!(
            jikkyo(&channel(1, "GR", "ＮＨＫ総合１", Some(999))?),
            Some(1)
        );
        assert_eq!(jikkyo(&channel(1, "GR", "未対応", Some(101))?), None);
        assert_eq!(jikkyo(&channel(1, "OTHER", "ＮＨＫ総合", Some(101))?), None);
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
                },
                Comment {
                    text: "new".into(),
                    origin: viewer_comments::Origin::Nx,
                    phase: viewer_comments::Phase::Live,
                    unix_seconds: 1,
                },
            ],
            |text| live.push(text.to_owned()),
        );
        assert_eq!(live, ["new"]);
        assert_eq!(comments.history.len(), 2);
        live.clear();
        comments.ingest(Vec::new(), |text| live.push(text.to_owned()));
        assert!(live.is_empty());
        assert_eq!(comments.history.len(), 2);
    }

    #[test]
    fn history_is_bounded_and_released_on_switch_and_disable()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut comments = Comments::default();
        let first = channel(1, "BS", "NHK", Some(101))?;
        let second = channel(2, "BS", "NHK", Some(102))?;
        comments.configure(true, Some(&first));
        comments.append((0..1000).map(|i| Comment {
            text: format!("<b>{i}</b>\n日本語").into_boxed_str(),
            origin: viewer_comments::Origin::Nx,
            phase: viewer_comments::Phase::Live,
            unix_seconds: i,
        }));
        let rows: Vec<serde_json::Value> = serde_json::from_str(&comments.json()?)?;
        assert_eq!(rows.len(), HISTORY_LIMIT);
        assert_eq!(rows[0]["text"], "<b>800</b>\n日本語");
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
        Ok(())
    }
}
