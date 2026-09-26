//! The catalog is generated before the application's Qt/C++ build script runs.
//! This crate is a build dependency, not part of the application's runtime.

pub const QRC_PATH: &str = concat!(env!("OUT_DIR"), "/translations.qrc");
