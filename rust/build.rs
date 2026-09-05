use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    println!("cargo:rerun-if-changed=../translations/app_ja.ts");
    println!("cargo:rerun-if-env-changed=QT_LRELEASE");
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let qm = output.join("ja.qm");
    let lrelease = std::env::var_os("QT_LRELEASE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            ["qtpaths6", "qtpaths"]
                .iter()
                .find_map(|tool| {
                    let result = std::process::Command::new(tool)
                        .args(["--query", "QT_HOST_BINS"])
                        .output()
                        .ok()?;
                    if !result.status.success() {
                        return None;
                    }
                    let path =
                        std::path::PathBuf::from(String::from_utf8(result.stdout).ok()?.trim())
                            .join(if cfg!(windows) {
                                "lrelease.exe"
                            } else {
                                "lrelease"
                            });
                    path.is_file().then_some(path)
                })
                .expect("Qt lrelease not found: install qt6-l10n-tools or set QT_LRELEASE")
        });
    assert!(
        std::process::Command::new(lrelease)
            .arg("../translations/app_ja.ts")
            .arg("-qm")
            .arg(&qm)
            .status()
            .expect("Could not run lrelease")
            .success(),
        "lrelease failed"
    );
    let qrc = output.join("translations.qrc");
    let qm_path = qm
        .to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    std::fs::write(&qrc, format!("<RCC><qresource prefix=\"/i18n\"><file alias=\"ja.qm\">{qm_path}</file></qresource></RCC>"))
        .expect("Could not write translation resource");
    CxxQtBuilder::new_qml_module(
        QmlModule::new("MirakurunViewer")
            .qml_file("../qml/Main.qml")
            .depend("QtQuick")
            .depend("QtQuick.Controls")
            .depend("QtQuick.Layouts"),
    )
    .qrc(&qrc)
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
        "../assets/icons/volume-x.svg",
        "../assets/icons/x.svg",
    ])
    .file("src/player.rs")
    .include_dir("src")
    .qt_module("Quick")
    .build();
}
