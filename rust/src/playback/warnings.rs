//! Fixed-size bus warning accounting. Never retain native messages or debug text.
use gstreamer as gst;

enum Kind {
    Other,
    ContinuityMismatch { pid: Option<u16> },
}

impl Kind {
    fn from_warning(warning: &gst::message::Warning) -> Self {
        let Some(details) = warning.details() else {
            return Self::Other;
        };
        if details.get::<&str>("warning-type").ok() != Some("continuity-mismatch") {
            return Self::Other;
        }
        // tsdemux publishes a guint MPEG-TS PID, whose valid range is 13 bits.
        // Missing or malformed details must not inherit the previous PID.
        let pid = details
            .get::<u32>("pid")
            .ok()
            .filter(|pid| *pid <= 0x1fff)
            .and_then(|pid| u16::try_from(pid).ok());
        Self::ContinuityMismatch { pid }
    }
}

/// Cumulative for the Playback owner's lifetime, including across channel changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub total: u64,
    pub continuity: u64,
    pub last_continuity_pid: Option<u16>,
}

impl Counts {
    pub fn observe(&mut self, warning: &gst::message::Warning) {
        self.total = self.total.saturating_add(1);
        if let Kind::ContinuityMismatch { pid } = Kind::from_warning(warning) {
            self.continuity = self.continuity.saturating_add(1);
            self.last_continuity_pid = pid;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gst::prelude::*;

    #[test]
    fn native_warning_details_are_typed_and_do_not_retain_the_source()
    -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        let source = gst::ElementFactory::make("identity").build()?;
        let weak = source.downgrade();
        let mut counts = Counts::default();
        {
            let message =
                gst::message::Warning::builder(gst::StreamError::Demux, "translated text")
                    .src(&source)
                    .details(
                        gst::Structure::builder("warning-details")
                            .field("warning-type", "continuity-mismatch")
                            .field("pid", 273_u32)
                            .build(),
                    )
                    .build();
            let gst::MessageView::Warning(warning) = message.view() else {
                return Err("fixture is not a warning".into());
            };
            counts.observe(warning);
        }
        drop(source);
        assert!(weak.upgrade().is_none());
        assert_eq!(
            counts,
            Counts {
                total: 1,
                continuity: 1,
                last_continuity_pid: Some(273)
            }
        );
        Ok(())
    }

    #[test]
    fn text_is_not_classification_and_invalid_pid_clears_previous_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        let mut counts = Counts {
            total: u64::MAX - 1,
            continuity: 1,
            last_continuity_pid: Some(273),
        };
        let ordinary = gst::message::Warning::new(gst::StreamError::Demux, "continuity-mismatch");
        let gst::MessageView::Warning(warning) = ordinary.view() else {
            return Err("fixture is not a warning".into());
        };
        counts.observe(warning);
        assert_eq!(counts.continuity, 1);
        for details in [
            gst::Structure::builder("warning-details")
                .field("warning-type", "continuity-mismatch")
                .build(),
            gst::Structure::builder("warning-details")
                .field("warning-type", "continuity-mismatch")
                .field("pid", "273")
                .build(),
            gst::Structure::builder("warning-details")
                .field("warning-type", "continuity-mismatch")
                .field("pid", 8192_u32)
                .build(),
        ] {
            let message = gst::message::Warning::builder(gst::StreamError::Demux, "warning")
                .details(details)
                .build();
            let gst::MessageView::Warning(warning) = message.view() else {
                return Err("fixture is not a warning".into());
            };
            counts.observe(warning);
            assert_eq!(counts.last_continuity_pid, None);
        }
        assert_eq!(counts.total, u64::MAX);
        assert_eq!(counts.continuity, 4);
        Ok(())
    }
}
