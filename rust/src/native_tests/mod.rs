//! Qt integration runners execute on the main thread, outside libtest workers.
// Qt meta-calls invoke generated signal methods outside Rust's call graph.
#[allow(dead_code)]
#[path = "../native_test_bridge.rs"]
mod bridge;
mod localization;
mod outline;
mod pointer;
mod startup;

pub fn run() -> i32 {
    let mut arguments = std::env::args().skip(2);
    let suite = arguments.next();
    if suite.as_deref() != Some("subtitle-rendering") && arguments.next().is_some() {
        eprintln!("Only subtitle-rendering accepts Qt Quick Test arguments");
        return 2;
    }
    match suite.as_deref() {
        Some("localization") => localization::run(false),
        Some("missing-catalog") => localization::run(true),
        Some("subtitle-outline") => outline::run(),
        Some("pointer-activity") => pointer::run(),
        Some("connection") => crate::player::connection_checks::run(),
        Some("startup") => startup::run(),
        Some("startup-window") => startup::run_window(),
        Some("subtitle-rendering") => {
            let mut qt_arguments = vec!["viewer-subtitle-tests".to_owned()];
            qt_arguments.extend(arguments);
            crate::danmaku_ui_tests::run_with_arguments(&qt_arguments)
        }
        _ => {
            eprintln!(
                "Expected --native-tests localization|missing-catalog|subtitle-outline|pointer-activity|subtitle-rendering|connection|startup"
            );
            2
        }
    }
}
