use cxx_qt_build::{CxxQtBuilder, QmlModule};

#[path = "build/translations.rs"]
mod translations;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let translations = translations::compile()?;
    let mut builder = CxxQtBuilder::new_qml_module(
        QmlModule::new("MinimalViewer")
            .qml_file("qml/Main.qml")
            .qml_file("qml/AudioSettings.qml")
            .qml_file("qml/PlaybackSettings.qml")
            .qml_file("qml/DanmakuAdjustments.qml")
            .qml_file("qml/StoppedPlayback.qml")
            .qml_file("qml/TextAction.qml")
            .qml_file("qml/PlaybackErrorDetails.qml")
            .qml_file("qml/WindowActions.qml")
            .qml_file("qml/WindowButtons.qml")
            .qml_file("qml/WindowDragArea.qml")
            .qml_file("qml/WindowResizeFrame.qml")
            .qml_file("qml/OverlayVisibility.qml")
            .qml_file("qml/SubtitleOverlay.qml")
            .qml_file("qml/SubtitleGlyph.qml")
            .qml_file("qml/ProgramGuide.qml")
            .qml_file("qml/GuideTimeline.qml")
            .qml_file("qml/GuideProgramDetails.qml")
            .qml_file("qml/GuideDateSelector.qml")
            .qml_file("qml/GuideToolbar.qml")
            .qml_file("qml/CurrentProgram.qml")
            .qml_file("qml/ProgramSidebar.qml")
            .qml_file("qml/CommentList.qml")
            .qml_file("qml/DanmakuOverlay.qml")
            .qml_file("qml/DanmakuTimeline.qml")
            .qml_file("qml/ToggleSwitch.qml")
            .qml_file("qml/SidebarChannels.qml")
            .qml_file("qml/SidebarTab.qml")
            .qml_file("qml/SidePanel.qml")
            .qml_file("qml/IconAction.qml")
            .qml_file("qml/ThemedSlider.qml")
            .qml_file("qml/VolumeSlider.qml")
            .qml_file("qml/SettingsDrawer.qml")
            .qml_file("qml/ProgramDetails.qml")
            .qml_file("qml/VideoStats.qml")
            .qml_file("qml/ChannelSelector.qml")
            .qml_file("qml/ChannelLogo.qml")
            .qml_file("qml/ChannelBrowser.qml")
            .qml_file("qml/ChannelWheelArea.qml")
            .qml_file("qml/AnimatedPanel.qml")
            .qml_file("qml/BroadcastTabs.qml")
            .qml_file("qml/BrowserCollapseButton.qml")
            .qml_file("qml/ChannelProgram.qml")
            .depend("QtQuick")
            .depend("QtQuick.Controls")
            .depend("QtQuick.Layouts")
            .depend("QtQuick.Shapes"),
    )
    .qrc(&translations)
    .qrc_resources([
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
        "../assets/icons/volume-x.svg",
        "../assets/icons/volume-2.svg",
        "../assets/icons/grid-2x2.svg",
        "../assets/icons/captions.svg",
        "../assets/icons/calendar-days.svg",
        "../assets/icons/settings-2.svg",
        "../assets/icons/maximize.svg",
        "../assets/fonts/rounded-mplus-1m-arib.ttf",
        "../assets/fonts/LICENSE-Rounded-Mplus-1m-for-ARIB.txt",
    ])
    .file("src/player.rs")
    .file("src/danmaku.rs")
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
