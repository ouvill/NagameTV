//! Startup probe on the session bus, independent of Qt and GIO.
use std::time::Duration;
use zbus::{Connection, zvariant::OwnedValue};

const TIMEOUT: Duration = Duration::from_secs(1);
const SERVICE: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const FILE_CHOOSER: &str = "org.freedesktop.portal.FileChooser";
const MINIMUM_CHOOSER_VERSION: u32 = 3;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("Could not initialize the desktop portal probe")]
    Runtime(#[source] std::io::Error),
    #[error("Could not connect to the session D-Bus")]
    Bus(#[source] zbus::Error),
    #[error("{operation} failed")]
    Call {
        operation: &'static str,
        #[source]
        source: zbus::Error,
    },
    #[error("The desktop portal did not answer within {TIMEOUT:?}")]
    Timeout,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum ChooserError {
    #[error("FileChooser portal is unavailable")]
    Missing(#[source] zbus::Error),
    #[error("FileChooser portal returned an invalid version")]
    InvalidVersion(#[source] zbus::Error),
    #[error("FileChooser portal version {0} does not support folders")]
    UnsupportedVersion(u32),
}

pub(super) fn probe() -> Result<Result<(), ChooserError>, Error> {
    // Joining the current-thread runtime before setting process defaults keeps
    // Qt and GIO from loading or starting their own persistent D-Bus threads.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(Error::Runtime)?;
    runtime.block_on(async {
        tokio::time::timeout(TIMEOUT, async {
            let connection = Connection::session().await.map_err(Error::Bus)?;
            // Qt blocks on Settings.ReadAll even if the FileChooser responds.
            // Both calls share the same deadline, including bus connection.
            let ((), chooser) =
                tokio::try_join!(read_settings(&connection), file_chooser(&connection))?;
            Ok(chooser)
        })
        .await
        .map_err(|_| Error::Timeout)?
    })
}

async fn read_settings(connection: &Connection) -> Result<(), Error> {
    let appearance = ["org.freedesktop.appearance"];
    match connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some(SETTINGS),
            "ReadAll",
            &(&appearance[..],),
        )
        .await
    {
        Ok(_) => Ok(()),
        // A portal without a Settings backend can still handle file selection.
        Err(error) if missing_interface(&error) => Ok(()),
        Err(source) => Err(Error::Call {
            operation: "Settings.ReadAll",
            source,
        }),
    }
}

async fn file_chooser(connection: &Connection) -> Result<Result<(), ChooserError>, Error> {
    let response = match connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(FILE_CHOOSER, "version"),
        )
        .await
    {
        Ok(response) => response,
        Err(error) if missing_interface(&error) => return Ok(Err(ChooserError::Missing(error))),
        Err(source) => {
            return Err(Error::Call {
                operation: "FileChooser version query",
                source,
            });
        }
    };
    Ok(response
        .body()
        .deserialize::<OwnedValue>()
        .and_then(|value| u32::try_from(value).map_err(Into::into))
        .map_err(ChooserError::InvalidVersion)
        .and_then(|version| {
            // Version 3 introduced folder selection for the screenshot picker.
            if version < MINIMUM_CHOOSER_VERSION {
                Err(ChooserError::UnsupportedVersion(version))
            } else {
                Ok(())
            }
        }))
}

fn missing_interface(error: &zbus::Error) -> bool {
    matches!(error, zbus::Error::MethodError(name, _, _) if matches!(
        name.as_str(),
        "org.freedesktop.DBus.Error.UnknownInterface"
            | "org.freedesktop.DBus.Error.UnknownMethod"
            | "org.freedesktop.DBus.Error.UnknownProperty"
    ))
}
