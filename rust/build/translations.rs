//! Compile the checked-in catalog into a build-local Qt resource.
use std::{error::Error, path::PathBuf, process::Command};

pub fn compile() -> Result<PathBuf, Box<dyn Error>> {
    println!("cargo:rerun-if-changed=../translations/app_ja.ts");
    println!("cargo:rerun-if-env-changed=QT_LRELEASE");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("Cargo did not provide OUT_DIR")?);
    let lrelease = match std::env::var_os("QT_LRELEASE") {
        Some(path) => PathBuf::from(path),
        None => ["qtpaths6", "qtpaths"]
            .into_iter()
            .find_map(|tool| {
                let result = Command::new(tool)
                    .args(["--query", "QT_HOST_BINS"])
                    .output()
                    .ok()?;
                if !result.status.success() {
                    return None;
                }
                let path = PathBuf::from(String::from_utf8(result.stdout).ok()?.trim()).join(
                    if cfg!(windows) {
                        "lrelease.exe"
                    } else {
                        "lrelease"
                    },
                );
                path.is_file().then_some(path)
            })
            .ok_or("Qt lrelease not found: install qt6-l10n-tools or set QT_LRELEASE")?,
    };
    let qm = output.join("ja.qm");
    let status = Command::new(lrelease)
        .arg("../translations/app_ja.ts")
        .arg("-qm")
        .arg(&qm)
        .status()?;
    if !status.success() {
        return Err(format!("lrelease failed: {status}").into());
    }
    let escaped = qm
        .to_str()
        .ok_or("Translation build path is not UTF-8")?
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let qrc = output.join("translations.qrc");
    std::fs::write(
        &qrc,
        format!(
            "<RCC><qresource prefix=\"/i18n\"><file alias=\"ja.qm\">{escaped}</file></qresource></RCC>"
        ),
    )?;
    Ok(qrc)
}
