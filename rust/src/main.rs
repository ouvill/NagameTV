mod audio;
mod build_info;
mod channel_model;
mod channels;
mod cli;
mod comment_model;
mod danmaku;
#[cfg(feature = "qml_tests")]
mod danmaku_ui_tests;
mod diagnostics;
mod epgstation;
mod error_log;
mod features;
mod json;
mod logging;
mod memory;
#[cfg(feature = "native_tests")]
mod native_tests;
#[cfg(target_os = "linux")]
mod platform;
mod playback;
mod player;
mod qt;
mod recording_model;
mod remote;
mod screenshots;
mod services;
mod settings;
mod shortcut_key;
mod transport;
mod video_file_model;
#[cfg(feature = "video_item_tests")]
mod video_item_tests;

fn main() -> std::process::ExitCode {
    // No platform, settings, diagnostics, Qt or GStreamer side effects before parsing.
    let command = match cli::Command::parse_from(std::env::args_os()) {
        Ok(command) => command,
        Err(error) => {
            let _ = error.print();
            return std::process::ExitCode::from(error.exit_code() as u8);
        }
    };
    let plan = match command {
        cli::Command::Launch(plan) => plan,
        cli::Command::BuildInfo => {
            println!("{}", build_info::json());
            return std::process::ExitCode::SUCCESS;
        }
        #[cfg(feature = "native_tests")]
        cli::Command::NativeTests(arguments) => {
            return std::process::ExitCode::from(native_tests::run(arguments).clamp(0, 255) as u8);
        }
        #[cfg(feature = "video_item_tests")]
        cli::Command::VideoItemTests => {
            return std::process::ExitCode::from(video_item_tests::run().clamp(0, 255) as u8);
        }
        #[cfg(feature = "qml_tests")]
        cli::Command::QmlTests => {
            return std::process::ExitCode::from(danmaku_ui_tests::run().clamp(0, 255) as u8);
        }
    };
    logging::init();
    tracing::info!(build_info = %build_info::json(), "Application build");
    // SAFETY: Logging creates no threads; Qt, GStreamer and workers have not started.
    #[cfg(target_os = "linux")]
    unsafe {
        platform::configure_at_startup();
    }
    // SAFETY: This is the main thread; no application workers have started.
    match unsafe { qt::application::load(plan) } {
        Ok(application) => {
            application.exec();
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            tracing::error!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
