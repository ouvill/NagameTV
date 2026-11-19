use super::ffi::Player;
use crate::{lan_discovery_model::ffi::LanDiscoveryModel, services::discovery::Kind};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl Player {
    pub fn lan_discovery_model(&self) -> *mut LanDiscoveryModel {
        self.rust().lan_discovery_model.as_ptr().cast_mut()
    }
    pub fn lan_search_kind(&self) -> QString {
        QString::from(self.rust().lan_search.kind().map_or("", Kind::code))
    }
    pub fn lan_search_status(&self) -> QString {
        QString::from(self.rust().lan_search.status())
    }

    pub fn search_lan(mut self: Pin<&mut Self>, kind: QString) {
        let Some(kind) = Kind::parse(&kind.to_string()) else {
            tracing::warn!("LAN search requested with an unknown service kind");
            return;
        };
        {
            let mut state = self.as_mut().rust_mut();
            let state = &mut *state;
            state.lan_search.request(state.network.as_ref(), kind);
        }
        self.publish_lan_search();
    }

    pub fn cancel_lan_search(mut self: Pin<&mut Self>, kind: QString) {
        let Some(kind) = Kind::parse(&kind.to_string()) else {
            return;
        };
        if self.rust().lan_search.kind() != Some(kind) {
            return;
        }
        self.as_mut().rust_mut().lan_search.cancel(kind);
        self.publish_lan_search();
    }

    pub(super) fn poll_lan_search(mut self: Pin<&mut Self>) {
        let changed = {
            let mut state = self.as_mut().rust_mut();
            let state = &mut *state;
            state
                .network
                .as_ref()
                .is_some_and(|network| state.lan_search.poll(network))
        };
        if changed {
            self.publish_lan_search();
        }
    }

    fn publish_lan_search(mut self: Pin<&mut Self>) {
        let rows = self.rust().lan_search.results().to_vec();
        self.as_mut()
            .rust_mut()
            .lan_discovery_model
            .pin_mut()
            .replace(rows);
        self.lan_search_changed();
    }
}
