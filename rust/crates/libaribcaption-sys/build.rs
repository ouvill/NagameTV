use std::{env, io, path::PathBuf};

fn cargo_path(name: &str) -> io::Result<PathBuf> {
    env::var_os(name).map(PathBuf::from).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Cargo did not provide {name}"),
        )
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed=ARIBCAPTION_SOURCE_DIR");
    println!("cargo:rerun-if-env-changed=LIBCLANG_PATH");
    println!("cargo:rerun-if-changed=wrapper.h");
    let manifest = cargo_path("CARGO_MANIFEST_DIR")?;
    let output = cargo_path("OUT_DIR")?.join("bindings.rs");
    let source = env::var_os("ARIBCAPTION_SOURCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../../third_party/libaribcaption"));
    let source = source.canonicalize().map_err(|error| {
        io::Error::new(error.kind(), format!(
            "Cannot resolve libaribcaption source {}: {error}; initialize git submodules or set ARIBCAPTION_SOURCE_DIR",
            source.display()
        ))
    })?;
    for entry in ["CMakeLists.txt", "cmake", "src", "include"] {
        println!("cargo:rerun-if-changed={}", source.join(entry).display());
    }
    let installed = cmake::Config::new(&source)
        .define("ARIBCC_NO_RENDERER", "ON")
        .define("ARIBCC_BUILD_TESTS", "OFF")
        .define("ARIBCC_SHARED_LIBRARY", "OFF")
        .define("CMAKE_INSTALL_LIBDIR", "lib")
        .build();
    println!(
        "cargo:rustc-link-search=native={}",
        installed.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=aribcaption");
    // Let cc select the C++ runtime for the target (MSVC, Apple, GNU, etc.).
    cc::Build::new()
        .cpp(true)
        .file("runtime.cpp")
        .compile("aribcaption_runtime");
    println!("cargo:rerun-if-changed=runtime.cpp");

    bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", installed.join("include").display()))
        .allowlist_type("aribcc_.*")
        .allowlist_function("aribcc_.*")
        .allowlist_var("ARIBCC_.*")
        .prepend_enum_name(false)
        .derive_default(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .map_err(|error| io::Error::other(format!("Cannot generate libaribcaption bindings: {error}; install libclang or set LIBCLANG_PATH")))?
        .write_to_file(&output)
        .map_err(|error| io::Error::new(error.kind(), format!("Cannot write {}: {error}", output.display())))?;
    Ok(())
}
