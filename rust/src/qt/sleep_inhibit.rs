//! Native portal ownership. Each request owns a dedicated bus connection so
//! cancellation also releases a grant whose method reply arrives too late.
use crate::sleep_inhibit::{Error, Inhibitor, Progress, Request};

pub struct Portal;
#[cfg(target_os = "linux")]
pub struct PortalRequest(cxx::UniquePtr<super::ffi::SleepInhibit>);
#[cfg(not(target_os = "linux"))]
pub struct PortalRequest;

impl Inhibitor for Portal {
    type Request = PortalRequest;
    fn acquire(&mut self) -> Result<PortalRequest, Error> {
        #[cfg(target_os = "linux")]
        {
            let reason = super::text::Text::source("Watching video").render();
            Ok(PortalRequest(super::ffi::acquire_sleep_inhibit(&reason)))
        }
        #[cfg(not(target_os = "linux"))]
        Err(Error {
            operation: "acquire portal sleep inhibition",
            unavailable: true,
            cause: "sleep inhibition is only implemented on Linux".into(),
        })
    }
}
impl Request for PortalRequest {
    fn poll(&mut self) -> Result<Progress, Error> {
        #[cfg(target_os = "linux")]
        {
            match self.0.pin_mut().poll() {
                0 => Ok(Progress::Pending),
                1 => Ok(Progress::Held),
                code => Err(Error {
                    operation: "acquire or maintain portal sleep inhibition",
                    unavailable: code == 2,
                    cause: self.0.error().to_string().into(),
                }),
            }
        }
        #[cfg(not(target_os = "linux"))]
        unreachable!("unsupported platforms cannot acquire a request")
    }
    fn release(mut self) -> Result<(), Error> {
        #[cfg(target_os = "linux")]
        self.0.pin_mut().release().map_err(|cause| Error {
            operation: "release portal sleep inhibition",
            unavailable: false,
            cause: Box::new(cause),
        })?;
        Ok(())
    }
}
impl Drop for PortalRequest {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if let Err(error) = self.0.pin_mut().release() {
            tracing::error!(
                operation = "release portal sleep inhibition on drop",
                error = &error as &dyn std::error::Error,
                "Sleep inhibition cleanup failed"
            );
        }
    }
}
