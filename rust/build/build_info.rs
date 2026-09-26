//! Application-specific build inputs, independent of changing Git state/time.
use std::{error::Error, path::PathBuf, process::Command};

pub fn generate() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("Missing OUT_DIR")?);
    let rustc = Command::new(std::env::var_os("RUSTC").ok_or("Missing RUSTC")?)
        .arg("--version")
        .output()?;
    if !rustc.status.success() {
        return Err(format!(
            "rustc --version failed: {}",
            String::from_utf8_lossy(&rustc.stderr)
        )
        .into());
    }
    let rustc = String::from_utf8(rustc.stdout)?;
    let mut features: Vec<_> = std::env::vars()
        .filter_map(|(key, _)| key.strip_prefix("CARGO_FEATURE_").map(str::to_owned))
        .collect();
    features.sort();
    let text = format!(
        "pub static INFO: viewer_diagnostics::build_info::BuildInfo = \n\
         viewer_diagnostics::build_info::BuildInfo {{\n\
         version: {version:?}, source: viewer_build_info::SOURCE,\n\
         built_unix_seconds: viewer_build_info::BUILT_UNIX_SECONDS,\n\
         target: {target:?}, profile: {profile:?}, rustc: {rustc:?}, features: &{features:?}\n}};\n",
        version = std::env::var("CARGO_PKG_VERSION")?,
        target = std::env::var("TARGET")?,
        profile = std::env::var("PROFILE")?,
        rustc = rustc.trim(),
    );
    std::fs::write(output.join("build_info.rs"), text)?;
    Ok(())
}
