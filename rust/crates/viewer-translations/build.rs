#[path = "../../build/translations.rs"]
mod translations;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").ok_or("Missing CARGO_MANIFEST_DIR")?,
    );
    // The private crate and source archive both keep this repository layout.
    translations::compile(&manifest.join("../../../translations/app_ja.ts"))?;
    Ok(())
}
