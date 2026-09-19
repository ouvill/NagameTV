use cxx_qt_build::{CxxQtBuilder, QmlModule};

#[path = "build/translations.rs"]
mod translations;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(feature = "distribution")
        && cfg!(any(
            feature = "evaluation-comment-list",
            feature = "evaluation-wide-comments",
            feature = "evaluation-collision-layout"
        ))
    {
        return Err(
            "Evaluation-only comment features cannot be included in distribution builds".into(),
        );
    }
    let translations = translations::compile()?;
    // The source directory is the module manifest. A new production component
    // is registered and compiled without a second hand-maintained file list.
    println!("cargo:rerun-if-changed=qml");
    let mut qml_files = std::fs::read_dir("qml")?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    qml_files.retain(|path| {
        path.extension().is_some_and(|extension| extension == "qml")
            // Remove the separate received-comment display from normal builds:
            // JP7080382 / JP7277651 / JP7153786 / JP7852687. Estimated expiry
            // 2027-03-02; registry status unverified, never auto-enable by date.
            // Hiding a compiled list at runtime would retain the display feature.
            && (cfg!(feature = "evaluation-comment-list")
                || path.file_name().is_none_or(|name| name != "CommentList.qml"))
    });
    qml_files.sort();
    let mut module = QmlModule::new("MinimalViewer");
    for path in &qml_files {
        module = module.qml_file(path);
    }
    let mut builder = CxxQtBuilder::new_qml_module(
        module
            .depend("QtCore")
            .depend("QtQuick")
            .depend("QtQuick.Dialogs")
            .depend("QtQuick.Controls")
            .depend("QtQuick.Layouts")
            .depend("QtQuick.Shapes")
            .depend("QtQuick.Effects"),
    )
    .qrc(&translations)
    .qrc_resources([
        "../assets/icons/camera.svg",
        "../assets/icons/folder-open.svg",
        "../assets/icons/info.svg",
        "../assets/icons/send.svg",
        "../assets/icons/pencil.svg",
        "../assets/icons/message-square.svg",
        "../assets/icons/chevron-down.svg",
        "../assets/icons/chevron-left.svg",
        "../assets/icons/panel-right-open.svg",
        "../assets/icons/minus.svg",
        "../assets/icons/x.svg",
        "../assets/icons/panel-right-close.svg",
        "../assets/icons/square.svg",
        "../assets/icons/play.svg",
        "../assets/icons/play-outline.svg",
        "../assets/icons/pause.svg",
        "../assets/icons/rotate-ccw.svg",
        "../assets/icons/rotate-cw.svg",
        "../assets/icons/tv.svg",
        "../assets/icons/recording.svg",
        "../assets/icons/message-square-off.svg",
        "../assets/icons/volume-x.svg",
        "../assets/icons/volume-2.svg",
        "../assets/icons/grid-2x2.svg",
        "../assets/icons/captions.svg",
        "../assets/icons/captions-off.svg",
        "../assets/icons/calendar-days.svg",
        "../assets/icons/settings-2.svg",
        "../assets/icons/maximize.svg",
        "../assets/icons/ellipsis.svg",
        "../assets/icons/radio.svg",
        "../assets/icons/radio-off.svg",
        "../assets/fonts/rounded-mplus-1m-arib.ttf",
        "../assets/fonts/LICENSE-Rounded-Mplus-1m-for-ARIB.txt",
    ])
    .file("src/player.rs")
    .file("src/screenshot_native.rs")
    .file("src/screenshot_overlay.rs")
    .file("src/danmaku.rs")
    .file("src/comment_model.rs")
    .include_dir("src")
    .qt_module("Quick");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        builder = builder.qt_module("DBus");
    }
    if std::env::var_os("CARGO_FEATURE_QML_TESTS").is_some() {
        builder = builder
            .qt_module("QuickTest")
            .file("src/danmaku_ui_tests.rs");
    }
    if std::env::var_os("CARGO_FEATURE_VIDEO_ITEM_TESTS").is_some() {
        builder = builder.file("src/video_item_tests.rs");
    }
    if std::env::var_os("CARGO_FEATURE_NATIVE_TESTS").is_some() {
        builder = builder
            .qt_module("Svg")
            .qt_module("Test")
            .file("src/native_test_bridge.rs");
    }
    builder.build();
    Ok(())
}
