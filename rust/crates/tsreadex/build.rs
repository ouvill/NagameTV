use std::{env, path::PathBuf};

fn main() {
    let source =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"))
            .join("../../../third_party/tsreadex");
    let mut build = cxx_build::bridge("src/lib.rs");
    build
        .cpp(true)
        .std("c++17")
        .include(&source)
        .include("src")
        .file("src/filter.cpp");
    for file in ["servicefilter.cpp", "util.cpp", "aac.cpp", "huffman.cpp"] {
        build.file(source.join(file));
    }
    build.compile("viewer_tsreadex");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed={}", source.display());
}
