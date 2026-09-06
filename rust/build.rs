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
            .qml_file("qml/ProgramDetails.qml")
            .qml_file("qml/VideoStats.qml")
            .qml_file("qml/ChannelSelector.qml")
            .depend("QtQuick")
            .depend("QtQuick.Controls")
            .depend("QtQuick.Layouts")
            .depend("QtQuick.Shapes"),
    )
    .qrc_resources([
        "../assets/fonts/rounded-mplus-1m-arib.ttf",
        "../assets/fonts/LICENSE-Rounded-Mplus-1m-for-ARIB.txt",
    ])
    .file("src/player.rs")
    .include_dir("src")
    .qt_module("Quick")
    .build();
}
