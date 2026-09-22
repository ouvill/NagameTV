//! Qt integration runners execute on the main thread, outside libtest workers.
// Qt meta-calls invoke generated signal methods outside Rust's call graph.
#[allow(dead_code)]
#[path = "../native_test_bridge.rs"]
mod bridge;
#[cfg(target_os = "linux")]
mod desktop_media;
mod localization;
mod outline;
mod pointer;
#[cfg(target_os = "linux")]
mod portal_dialogs;
mod recording_audit;
#[cfg(target_os = "linux")]
mod recording_drop;
mod screenshots;
mod startup;
mod timeshift;
mod video_processing;

use clap::{Parser, Subcommand};
use std::{ffi::OsString, path::PathBuf};

#[derive(Parser)]
#[command(name = "nagametv --native-tests")]
struct Arguments {
    #[command(subcommand)]
    suite: Suite,
}

#[derive(Subcommand)]
enum Suite {
    Localization,
    MissingCatalog,
    SubtitleOutline,
    PointerActivity,
    #[cfg(target_os = "linux")]
    DesktopMedia,
    Connection,
    Startup,
    StartupWindow,
    Timeshift,
    VideoProcessing,
    ScreenshotPlayback,
    RecordingPidChange,
    RecordingAudit {
        path: PathBuf,
    },
    RecordingProbe {
        path: PathBuf,
    },
    #[cfg(target_os = "linux")]
    PortalDialogs,
    SubtitleRendering {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<String>,
    },
    Screenshots {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<String>,
    },
}

pub fn run(arguments: Vec<OsString>) -> i32 {
    let args = match Arguments::try_parse_from(
        std::iter::once(OsString::from("nagametv --native-tests")).chain(arguments),
    ) {
        Ok(args) => args,
        Err(error) => {
            let _ = error.print();
            return error.exit_code();
        }
    };
    match args.suite {
        Suite::Localization => localization::run(false),
        Suite::MissingCatalog => localization::run(true),
        Suite::SubtitleOutline => outline::run(),
        Suite::PointerActivity => pointer::run(),
        #[cfg(target_os = "linux")]
        Suite::DesktopMedia => desktop_media::run(),
        Suite::Connection => crate::player::connection_checks::run(),
        Suite::Startup => startup::run(),
        Suite::StartupWindow => startup::run_window(),
        Suite::Timeshift => startup::run_timeshift(),
        Suite::VideoProcessing => startup::run_video_processing(),
        Suite::ScreenshotPlayback => startup::run_screenshots(),
        Suite::RecordingPidChange => startup::run_pid_change(),
        Suite::RecordingAudit { path } => startup::run_recording_audit(path),
        Suite::RecordingProbe { path } => startup::run_recording_probe(path),
        #[cfg(target_os = "linux")]
        Suite::PortalDialogs => portal_dialogs::run(),
        Suite::SubtitleRendering { arguments } => {
            let mut qt_arguments = vec!["viewer-subtitle-tests".to_owned()];
            qt_arguments.extend(arguments);
            crate::danmaku_ui_tests::run_with_arguments(&qt_arguments)
        }
        Suite::Screenshots { arguments } => {
            crate::features::PLAN
                .set(crate::features::LaunchPlan::Restricted(
                    "none".parse().expect("test plan"),
                ))
                .expect("test plan initialized once");
            let mut qt_arguments = vec!["viewer-screenshot-tests".to_owned()];
            qt_arguments.extend(arguments);
            crate::danmaku_ui_tests::run_with_arguments(&qt_arguments)
        }
    }
}
