//! CEC 1.4 routing and follower replies for a Playback device.

pub(super) const BROADCAST: u8 = 15;
const TV: u8 = 0;
pub(super) const MAX_MESSAGE_SIZE: usize = 16;
pub(super) const USER_CONTROL_PRESSED: u8 = 0x44;
pub(super) const USER_CONTROL_RELEASED: u8 = 0x45;
const FEATURE_ABORT: u8 = 0x00;
const ROUTING_CHANGE: u8 = 0x80;
const ACTIVE_SOURCE: u8 = 0x82;
const REQUEST_ACTIVE_SOURCE: u8 = 0x85;
const SET_STREAM_PATH: u8 = 0x86;
const MENU_REQUEST: u8 = 0x8d;
const MENU_STATUS: u8 = 0x8e;
const GIVE_DEVICE_POWER_STATUS: u8 = 0x8f;
const REPORT_POWER_STATUS: u8 = 0x90;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Frame {
    bytes: [u8; MAX_MESSAGE_SIZE],
    len: usize,
}

impl Frame {
    pub(super) fn parse(bytes: &[u8]) -> Option<Self> {
        if !(2..=MAX_MESSAGE_SIZE).contains(&bytes.len()) {
            return None;
        }
        let mut frame = Self {
            bytes: [0; MAX_MESSAGE_SIZE],
            len: bytes.len(),
        };
        frame.bytes[..bytes.len()].copy_from_slice(bytes);
        Some(frame)
    }

    fn new(source: u8, destination: u8, opcode: u8, operands: &[u8]) -> Self {
        let mut frame = Self {
            bytes: [0; MAX_MESSAGE_SIZE],
            len: operands.len() + 2,
        };
        frame.bytes[0] = source << 4 | destination;
        frame.bytes[1] = opcode;
        frame.bytes[2..frame.len].copy_from_slice(operands);
        frame
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    pub(super) fn source(self) -> u8 {
        self.bytes[0] >> 4
    }

    pub(super) fn destination(self) -> u8 {
        self.bytes[0] & 0x0f
    }

    pub(super) fn opcode(self) -> u8 {
        self.bytes[1]
    }

    pub(super) fn operands(&self) -> &[u8] {
        &self.bytes[2..self.len]
    }
}

#[derive(Clone, Copy)]
enum AbortReason {
    UnrecognizedOpcode = 0,
    InvalidOperand = 3,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
enum Menu {
    #[default]
    Active = 0,
    Inactive = 1,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Route {
    #[default]
    Other,
    Active,
}

/// A route belongs to one acquired physical and logical address. A change of
/// either address invalidates the old source state.
pub(super) struct PlaybackSource {
    physical: u16,
    logical: u8,
    route: Route,
    menu: Menu,
    playback_visible: bool,
}

impl PlaybackSource {
    pub(super) fn new(physical: u16, logical: u8) -> Self {
        Self {
            physical,
            logical,
            route: Route::Other,
            menu: Menu::Active,
            playback_visible: false,
        }
    }

    pub(super) fn update_visibility(&mut self, visible: bool) -> Option<Frame> {
        let became_visible = visible && !self.playback_visible;
        self.playback_visible = visible;
        (became_visible && self.route == Route::Other).then(|| self.active_source())
    }

    pub(super) fn receive(&mut self, frame: Frame) -> Option<Frame> {
        if frame.source() == self.logical {
            return None;
        }
        if frame.destination() == BROADCAST {
            return match (frame.opcode(), frame.operands()) {
                (ROUTING_CHANGE, [_, _, high, low]) | (SET_STREAM_PATH, [high, low]) => {
                    if u16::from_be_bytes([*high, *low]) == self.physical {
                        Some(self.active_source())
                    } else {
                        self.route = Route::Other;
                        None
                    }
                }
                (ACTIVE_SOURCE, [_, _]) => {
                    self.route = Route::Other;
                    None
                }
                // This broadcast has no physical-address operand. Only the
                // source already selected on the bus is meant to answer.
                (REQUEST_ACTIVE_SOURCE, []) if self.route == Route::Active => {
                    Some(self.active_source())
                }
                _ => None,
            };
        }
        if frame.destination() != self.logical || frame.source() == BROADCAST {
            return None;
        }
        let reply = |opcode, operands: &[u8]| {
            Some(Frame::new(self.logical, frame.source(), opcode, operands))
        };
        let abort = |reason: AbortReason| reply(FEATURE_ABORT, &[frame.opcode(), reason as u8]);
        match (frame.opcode(), frame.operands()) {
            (GIVE_DEVICE_POWER_STATUS, []) => reply(REPORT_POWER_STATUS, &[0]),
            (MENU_REQUEST, [request @ 0..=2]) => {
                self.menu = match request {
                    0 => Menu::Active,
                    1 => Menu::Inactive,
                    _ => self.menu,
                };
                reply(MENU_STATUS, &[self.menu as u8])
            }
            (GIVE_DEVICE_POWER_STATUS | MENU_REQUEST, _) => abort(AbortReason::InvalidOperand),
            // A Feature Abort ends the exchange. The Linux CEC core owns the
            // identity queries and the test-only Abort in non-passthrough mode.
            (FEATURE_ABORT | 0x83 | 0x8c | 0x46 | 0x9f | 0xff, _) => None,
            _ => abort(AbortReason::UnrecognizedOpcode),
        }
    }

    fn active_source(&self) -> Frame {
        Frame::new(
            self.logical,
            BROADCAST,
            ACTIVE_SOURCE,
            &self.physical.to_be_bytes(),
        )
    }

    pub(super) fn sent(&mut self, frame: Frame) -> Option<Frame> {
        if frame.opcode() != ACTIVE_SOURCE {
            return None;
        }
        self.route = Route::Active;
        // CEC 1.4 §13.12.2: the TV assumes the new source has no active menu
        // unless Menu Status follows Active Source. The viewer accepts D-pad
        // input for its on-screen controls even before video starts.
        (self.menu == Menu::Active)
            .then(|| Frame::new(self.logical, TV, MENU_STATUS, &[self.menu as u8]))
    }
}
