use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("MinimalViewer")
            .qml_file("qml/Main.qml")
            .qml_file("qml/WindowActions.qml")
            .qml_file("qml/OverlayVisibility.qml")
            .qml_file("qml/SubtitleOverlay.qml")
            .qml_file("qml/SubtitleGlyph.qml")
            .qml_file("qml/ProgramGuide.qml")
            .qml_file("qml/CurrentProgram.qml")
            .qml_file("qml/IconAction.qml")
            .qml_file("qml/ThemedSlider.qml")
            .qml_file("qml/SettingsPopup.qml")
            .qml_file("qml/ProgramDetails.qml")
            .qml_file("qml/VideoStats.qml")
            .qml_file("qml/ChannelSelector.qml")
            .qml_file("qml/ChannelLogo.qml")
            .qml_file("qml/ChannelBrowser.qml")
            .qml_file("qml/AnimatedPanel.qml")
            .qml_file("qml/BroadcastTabs.qml")
            .qml_file("qml/BrowserCollapseButton.qml")
            .qml_file("qml/ChannelProgram.qml")
            .depend("QtQuick")
            .depend("QtQuick.Controls")
            .depend("QtQuick.Layouts")
            .depend("QtQuick.Shapes"),
    )
    .qrc_resources([
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
    .include_dir("src")
    .qt_module("Quick")
    .build();
}
