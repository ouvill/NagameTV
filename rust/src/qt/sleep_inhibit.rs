//! Native leases and environment notifications for Rust-owned routing.
//! Dedicated connections also release grants whose replies arrive after cancellation.
use crate::sleep_inhibit::{
    Error, Progress, Request,
    routes::{Kind, Routes, Transport},
};

pub type Portal = Routes<Native>;
pub struct Native {
    #[cfg(target_os = "linux")]
    environment: cxx::UniquePtr<super::ffi::InhibitEnvironment>,
}
impl Default for Native {
    fn default() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            environment: cxx::UniquePtr::null(),
        }
    }
}
impl Default for Portal {
    fn default() -> Self {
        Self::new(Native::default())
    }
}
#[cfg(target_os = "linux")]
pub struct PortalRequest(cxx::UniquePtr<super::ffi::SleepInhibit>);
#[cfg(not(target_os = "linux"))]
pub struct PortalRequest;

impl Transport for Native {
    type Lease = PortalRequest;
    fn acquire(&mut self, kind: Kind) -> Result<PortalRequest, Error> {
        #[cfg(target_os = "linux")]
        {
            if self.environment.is_null() {
                self.environment = super::ffi::make_inhibit_environment();
            }
            if kind == Kind::Logind {
                self.environment.pin_mut().observe_system();
            }
            let reason = super::text::Text::source("Watching video").render();
            Ok(PortalRequest(super::ffi::acquire_sleep_inhibit(
                &kind.as_str().into(),
                &reason,
            )))
        }
        #[cfg(not(target_os = "linux"))]
        Err(Error {
            operation: "acquire portal sleep inhibition",
            unavailable: true,
            cause: "sleep inhibition is only implemented on Linux".into(),
        })
    }
    fn revision_for(&mut self, kind: Kind) -> u64 {
        #[cfg(target_os = "linux")]
        if !self.environment.is_null() {
            return self.environment.revision_for(&kind.as_str().into());
        }
        0
    }
    fn revision(&mut self) -> u64 {
        #[cfg(target_os = "linux")]
        if !self.environment.is_null() {
            return self.environment.revision();
        }
        0
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
                    operation: "acquire or maintain desktop sleep inhibition",
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
            operation: "release desktop sleep inhibition",
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
                operation = "release desktop sleep inhibition on drop",
                error = &error as &dyn std::error::Error,
                "Sleep inhibition cleanup failed"
            );
        }
    }
}
