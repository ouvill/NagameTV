//! Playback-independent commentary selection and reception.
pub mod activity;
mod mapping;
pub(crate) mod replay;
use crate::{channels::Channel, services::Network};
use viewer_comments::{
    Comment,
    connection::{Endpoints, State},
    controller::{Controller, Status},
};

/// Borrow the failure from its connection generation; presentation owns no error copy.
#[derive(Debug)]
pub enum PresentationStatus<'a> {
    Disabled,
    WaitingForChannel,
    Unavailable,
    Connecting,
    Receiving,
    Failed(&'a viewer_comments::connection::Error),
    Retrying(Option<&'a viewer_comments::connection::Error>),
}

#[derive(Default)]
pub struct Comments {
    controller: Controller,
    channel_selected: bool,
    target: Option<(u64, u16)>,
    pub posting: viewer_comments::posting::Controller,
}

fn jikkyo(channel: &Channel) -> Option<u16> {
    let service = channel.broadcast?;
    mapping::resolve(service.network_id, service.service_id)
}
pub(crate) fn channel_for(service: crate::channels::BroadcastService) -> Option<u16> {
    mapping::resolve(service.network_id, service.service_id)
}

impl Comments {
    pub fn reception_epoch(&self) -> Option<u64> {
        self.controller.reception_epoch()
    }
    pub fn configure(&mut self, enabled: bool, channel: Option<&Channel>) -> bool {
        self.channel_selected = channel.is_some();
        let target = enabled
            .then_some(channel)
            .flatten()
            .and_then(|channel| jikkyo(channel).map(|id| (channel.id, id)));
        if self.target == target {
            return false;
        }
        self.target = target;
        self.posting.configure(
            target.map(|(service_id, id)| viewer_comments::posting::Target {
                service_id,
                watch_url: format!(
                    "wss://nx-jikkyo.tsukumijima.net/api/v1/channels/jk{id}/ws/watch"
                ),
            }),
        );
        self.controller.configure(target.map(|(_, id)| Endpoints {
            threads: format!("https://nx-jikkyo.tsukumijima.net/api/v1/channels/jk{id}/threads"),
            comments: format!("wss://nx-jikkyo.tsukumijima.net/api/v1/channels/jk{id}/ws/comment"),
        }));
        true
    }

    pub fn jikkyo_id(&self) -> Option<u16> {
        self.target.map(|(_, id)| id)
    }

    pub fn poll(
        &mut self,
        network: &Network,
    ) -> Result<Vec<Comment>, viewer_comments::controller::Error> {
        network.poll_comments(&mut self.controller)
    }

    pub fn status(&self, enabled: bool) -> PresentationStatus<'_> {
        if !enabled {
            return PresentationStatus::Disabled;
        }
        if !self.channel_selected {
            return PresentationStatus::WaitingForChannel;
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
    fn status_distinguishes_no_selection_from_an_unsupported_channel()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut comments = Comments::default();
        let unsupported = channel(1, "BS", "Other", None)?;
        let supported = channel(2, "BS", "NHK", Some(101))?;
        assert!(matches!(
            comments.status(true),
            PresentationStatus::WaitingForChannel
        ));
        // These transitions have no reception target but must still update the status.
        assert!(!comments.configure(true, Some(&unsupported)));
        assert!(matches!(
            comments.status(true),
            PresentationStatus::Unavailable
        ));
        assert!(!comments.configure(true, None));
        assert!(matches!(
            comments.status(true),
            PresentationStatus::WaitingForChannel
        ));
        assert!(comments.configure(true, Some(&supported)));
        assert!(matches!(
            comments.status(true),
            PresentationStatus::Connecting
        ));
        assert!(comments.configure(false, Some(&supported)));
        assert!(matches!(
            comments.status(false),
            PresentationStatus::Disabled
        ));
        Ok(())
    }

    #[test]
    fn switching_and_disabling_report_history_invalidation()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut comments = Comments::default();
        let first = channel(1, "BS", "NHK", Some(101))?;
        let second = channel(2, "BS", "NHK", Some(102))?;
        assert!(comments.configure(true, Some(&first)));
        assert!(!comments.configure(true, Some(&first)));
        // Distinct broadcasts sharing jk101 must still clear history.
        assert!(comments.configure(true, Some(&second)));
        assert!(comments.configure(false, Some(&second)));
        assert!(!comments.configure(false, None));
        assert!(comments.target.is_none());
        assert!(comments.controller.is_stopped());
        assert!(matches!(
            comments.status(false),
            PresentationStatus::Disabled
        ));
        Ok(())
    }
}
