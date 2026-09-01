use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("MirakurunViewer")
            .qml_file("../qml/Main.qml")
            .depend("QtQuick")
            .depend("QtQuick.Controls")
            .depend("QtQuick.Layouts"),
    )
    .file("src/player.rs")
    .include_dir("src")
    .qt_module("Quick")
    .build();
}
