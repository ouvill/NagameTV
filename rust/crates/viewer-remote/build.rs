fn main() -> Result<(), Box<dyn std::error::Error>> {
    embed_web()?;
    let mut prost = tonic_prost_build::Config::new();
    prost.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);
    tonic_prost_build::configure()
        .file_descriptor_set_path(
            std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("viewer.bin"),
        )
        .compile_with_config(prost, &["proto/viewer/v1/player.proto"], &["proto"])?;
    Ok(())
}

fn embed_web() -> Result<(), Box<dyn std::error::Error>> {
    use std::{fs, path::Path};
    let root = Path::new("web/dist").canonicalize()?;
    println!("cargo:rerun-if-changed=web/dist");
    fn collect(directory: &Path, files: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                collect(&entry.path(), files)?;
            } else if entry.file_type()?.is_file() {
                files.push(entry.path());
            }
        }
        Ok(())
    }
    let mut files = vec![root.join("index.html")];
    let assets = root.join("assets");
    if assets.is_dir() {
        collect(&assets, &mut files)?;
    }
    files.sort();
    let mut code = String::from("static ASSETS: &[Asset] = &[\n");
    for file in files {
        let path = file
            .strip_prefix(&root)?
            .to_str()
            .ok_or("asset path is not UTF-8")?
            .replace('\\', "/");
        let content_type = match file.extension().and_then(|value| value.to_str()) {
            Some("html") => "text/html; charset=utf-8",
            Some("js" | "mjs") => "text/javascript; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            Some("json" | "map") => "application/json",
            Some("svg") => "image/svg+xml",
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("webp") => "image/webp",
            Some("ico") => "image/x-icon",
            Some("woff2") => "font/woff2",
            Some("woff") => "font/woff",
            Some("wasm") => "application/wasm",
            _ => "application/octet-stream",
        };
        // esbuild dist uses app-HASH.ext. Plain filenames remain revalidated.
        let immutable = path.starts_with("assets/")
            && file
                .file_stem()
                .and_then(|value| value.to_str())
                .and_then(|stem| stem.rsplit_once('-'))
                .is_some_and(|(_, hash)| {
                    hash.len() >= 8 && hash.bytes().all(|c| c.is_ascii_alphanumeric())
                });
        code.push_str(&format!("Asset {{ path: {:?}, contents: include_bytes!({:?}), content_type: {:?}, immutable: {} }},\n", format!("/{path}"), file, content_type, immutable));
    }
    code.push_str("];\n");
    fs::write(
        Path::new(&std::env::var("OUT_DIR")?).join("web_assets.rs"),
        code,
    )?;
    Ok(())
}
