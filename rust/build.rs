use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("MirakurunViewer")
            .qml_file("../qml/Main.qml")
            .depend("QtQuick")
            .depend("QtQuick.Controls")
            .depend("QtQuick.Layouts"),
    )
    .qrc_resources([
        "../assets/icons/calendar-days.svg",
        "../assets/icons/captions.svg",
        "../assets/icons/chevron-down.svg",
        "../assets/icons/chevron-left.svg",
        "../assets/icons/grid-2x2.svg",
        "../assets/icons/info.svg",
        "../assets/icons/maximize.svg",
        "../assets/icons/message-square.svg",
        "../assets/icons/minus.svg",
        "../assets/icons/panel-right-close.svg",
        "../assets/icons/panel-right-open.svg",
        "../assets/icons/pause.svg",
        "../assets/icons/pencil.svg",
        "../assets/icons/play.svg",
        "../assets/icons/send.svg",
        "../assets/icons/settings-2.svg",
        "../assets/icons/square.svg",
        "../assets/icons/volume-2.svg",
        "../assets/icons/x.svg",
    ])
    .file("src/player.rs")
    .include_dir("src")
    .qt_module("Quick")
    .build();
}
