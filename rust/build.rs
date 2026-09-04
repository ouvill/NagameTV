use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    println!("cargo:rerun-if-changed=../third_party/libaribcaption/CMakeLists.txt");
    println!("cargo:rerun-if-changed=../third_party/libaribcaption/src");
    println!("cargo:rerun-if-changed=../third_party/libaribcaption/include");
    let aribcaption = cmake::Config::new("../third_party/libaribcaption")
        .define("ARIBCC_NO_RENDERER", "ON")
        .define("ARIBCC_BUILD_TESTS", "OFF")
        .define("ARIBCC_SHARED_LIBRARY", "OFF")
        .build();
    println!(
        "cargo:rustc-link-search=native={}/lib",
        aribcaption.display()
    );
    println!("cargo:rustc-link-lib=static=aribcaption");
    println!("cargo:rustc-link-lib=dylib=stdc++");

    CxxQtBuilder::new_qml_module(
        QmlModule::new("MirakurunViewer")
            .qml_file("../qml/Main.qml")
            .depend("QtQuick")
            .depend("QtQuick.Controls")
            .depend("QtQuick.Layouts"),
    )
    .qrc_resources([
        "../assets/fonts/rounded-mplus-1m-arib.ttf",
        "../assets/fonts/LICENSE-Rounded-Mplus-1m-for-ARIB.txt",
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
