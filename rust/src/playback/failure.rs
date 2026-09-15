//! Stable user guidance is separate from native diagnostic text and Qt translation.
use super::Error;
use gstreamer as gst;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hint {
    ServiceUnavailable,
    MissingChannel,
    AccessDenied,
    Timeout,
    Server,
    Rejected,
    Network,
    Ended,
    Generic,
    Recording,
}

impl Hint {
    pub fn source(self) -> &'static str {
        match self {
            Self::ServiceUnavailable => {
                "The stream is temporarily unavailable. The server or tuners may be busy or unavailable. Wait a moment and try again, or check the server."
            }
            Self::MissingChannel => {
                "This channel was not found on Mirakurun. Refresh the channel list and choose a channel again."
            }
            Self::AccessDenied => {
                "Mirakurun denied access to the stream. Check the server's access settings."
            }
            Self::Timeout => {
                "The server did not respond in time. Check the connection and try again."
            }
            Self::Server => "Mirakurun could not start the stream. Check the server and try again.",
            Self::Rejected => {
                "Mirakurun rejected the stream request. Check the connection settings and channel."
            }
            Self::Network => {
                "Could not receive the stream from Mirakurun. Check the server and network connection, then try again."
            }
            Self::Ended => "The live stream ended unexpectedly. Try again to reconnect.",
            Self::Generic => {
                "Could not play this channel. Try again or choose another channel. See the error details if the problem continues."
            }
            Self::Recording => {
                "Could not play this TS file. Check that it is readable and contains supported video and audio."
            }
        }
    }
}

impl Error {
    pub fn hint(&self) -> Hint {
        match self {
            Self::Recording(_) => Hint::Recording,
            // Cleanup must not hide the reason the original stream failed.
            Self::Cleanup { primary, .. } => primary.hint(),
            Self::EndOfStream => Hint::Ended,
            // HTTP 503 can originate at a proxy or an overloaded server too;
            // the status alone does not establish Mirakurun tuner exhaustion.
            Self::Stream {
                http_status: Some(503),
                ..
            } => Hint::ServiceUnavailable,
            Self::Stream {
                http_status: Some(404),
                ..
            } => Hint::MissingChannel,
            Self::Stream {
                http_status: Some(401 | 403),
                ..
            } => Hint::AccessDenied,
            Self::Stream {
                http_status: Some(408 | 504),
                ..
            } => Hint::Timeout,
            Self::Stream {
                http_status: Some(500..=599),
                ..
            } => Hint::Server,
            Self::Stream {
                http_status: Some(400..=499),
                ..
            } => Hint::Rejected,
            Self::Stream {
                source,
                network_source: true,
                ..
            } if source.kind::<gst::ResourceError>().is_some() => Hint::Network,
            _ => Hint::Generic,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn structured_http_status_survives_native_message_and_cleanup() -> TestResult {
        gst::init()?;
        for (code, expected) in [
            (503, Hint::ServiceUnavailable),
            (404, Hint::MissingChannel),
            (401, Hint::AccessDenied),
            (403, Hint::AccessDenied),
            (408, Hint::Timeout),
            (504, Hint::Timeout),
            (500, Hint::Server),
            (599, Hint::Server),
            (400, Hint::Rejected),
            (429, Hint::Rejected),
            (200, Hint::Generic),
            (600, Hint::Generic),
        ] {
            let message =
                gst::message::Error::builder(gst::ResourceError::Read, "arbitrary native language")
                    .details(
                        gst::Structure::builder("details")
                            .field("http-status-code", code as u32)
                            .build(),
                    )
                    .debug("diagnostic 404 is not the HTTP code")
                    .build();
            let gst::MessageView::Error(native) = message.view() else {
                return Err("expected error message".into());
            };
            let error = super::super::stream_error(native);
            assert_eq!(error.hint(), expected);
            assert!(error.to_string().contains(&format!("HTTP: Some({code})")));
            let combined = Error::Cleanup {
                primary: Box::new(error),
                cleanup: Box::new(Error::Unavailable),
            };
            assert_eq!(combined.hint(), expected);
            assert!(combined.to_string().contains("diagnostic 404"));
        }
        Ok(())
    }

    #[test]
    fn network_provenance_and_optional_details_do_not_mislabel_output_failure() -> TestResult {
        gst::init()?;
        // Elements remain in NULL: no requests, playback or hardware access.
        let http = gst::ElementFactory::make("souphttpsrc").build()?;
        for details in [
            None,
            Some(
                gst::Structure::builder("details")
                    .field("http-status-code", "503")
                    .build(),
            ),
        ] {
            let message = gst::message::Error::builder(gst::ResourceError::OpenRead, "HTTP 503")
                .details_if_some(details.clone())
                .build();
            let gst::MessageView::Error(native) = message.view() else {
                return Err("expected error message".into());
            };
            assert_eq!(super::super::stream_error(native).hint(), Hint::Generic);
            let message =
                gst::message::Error::builder(gst::ResourceError::OpenRead, "connection refused")
                    .src(&http)
                    .details_if_some(details)
                    .build();
            let gst::MessageView::Error(native) = message.view() else {
                return Err("expected error message".into());
            };
            assert_eq!(super::super::stream_error(native).hint(), Hint::Network);
        }
        assert_eq!(Error::EndOfStream.hint(), Hint::Ended);
        Ok(())
    }
}
