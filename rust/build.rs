use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("MinimalViewer")
            .qml_file("qml/Main.qml")
            .qml_file("qml/SubtitleOverlay.qml")
            .qml_file("qml/ProgramGuide.qml")
            .qml_file("qml/VideoStats.qml")
            .qml_file("qml/ChannelSelector.qml")
            .depend("QtQuick")
            .depend("QtQuick.Controls")
            .depend("QtQuick.Layouts"),
    )
    .file("src/player.rs")
    .include_dir("src")
    .qt_module("Quick")
    .build();
}
