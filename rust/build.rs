use cxx_qt_build::{CxxQtBuilder, QmlModule};

#[path = "build/translations.rs"]
mod translations;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let translations = translations::compile()?;
    // The source directory is the module manifest. A new production component
    // is registered and compiled without a second hand-maintained file list.
    println!("cargo:rerun-if-changed=qml");
    let mut qml_files = std::fs::read_dir("qml")?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    qml_files.retain(|path| path.extension().is_some_and(|extension| extension == "qml"));
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
        "../assets/fonts/rounded-mplus-1m-arib.ttf",
        "../assets/fonts/LICENSE-Rounded-Mplus-1m-for-ARIB.txt",
    ])
    .file("src/player.rs")
    .file("src/danmaku.rs")
    .file("src/comment_model.rs")
    .include_dir("src")
    .qt_module("Quick");
    if std::env::var_os("CARGO_FEATURE_QML_TESTS").is_some() {
        builder = builder
            .qt_module("QuickTest")
            .file("src/danmaku_ui_tests.rs");
    }
    if std::env::var_os("CARGO_FEATURE_VIDEO_ITEM_TESTS").is_some() {
        builder = builder.file("src/video_item_tests.rs");
    }
    if std::env::var_os("CARGO_FEATURE_NATIVE_TESTS").is_some() {
        builder = builder.qt_module("Svg").file("src/native_test_bridge.rs");
    }
    builder.build();
    Ok(())
}
