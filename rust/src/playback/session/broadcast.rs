//! Optional reception belongs to one TS input. Starting it cannot fail playback.
use crate::{features::data_broadcast, playback};

pub(super) struct Broadcast {
    service: u16,
    original_network_id: Option<u16>,
    receiver: Receiver,
}

enum Receiver {
    Disabled,
    Running(data_broadcast::Session),
    // Periodic projection must not repeat a failed initialization. Disabling
    // the feature or replacing the input permits an explicit retry.
    Failed,
}

impl Broadcast {
    pub(super) fn new(service: u16, original_network_id: Option<u16>) -> Self {
        Self {
            service,
            original_network_id,
            receiver: Receiver::Disabled,
        }
    }

    pub(super) fn session(&self) -> Option<&data_broadcast::Session> {
        match &self.receiver {
            Receiver::Running(session) => Some(session),
            Receiver::Disabled | Receiver::Failed => None,
        }
    }

    pub(super) fn configure(
        &mut self,
        source: &mut playback::input::Input,
        enabled: bool,
    ) -> playback::Result<()> {
        match (&self.receiver, enabled) {
            (Receiver::Disabled, true) => {
                self.receiver = Receiver::Failed;
                let session =
                    data_broadcast::Session::start(self.service, self.original_network_id)?;
                source.set_data_broadcast(Some(session.tap()))?;
                self.receiver = Receiver::Running(session);
            }
            (Receiver::Running(_) | Receiver::Failed, false) => {
                // Drop first: even a poisoned reader cannot leave reception or
                // browser connections alive. Detach the now-inactive tap next.
                self.receiver = Receiver::Disabled;
                source.set_data_broadcast(None)?;
            }
            (Receiver::Disabled, false) | (Receiver::Running(_) | Receiver::Failed, true) => {}
        }
        Ok(())
    }
}
