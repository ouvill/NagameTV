//! Low-level, unsafe bindings generated from the headers of the library we build.
//! Prefer the `libaribcaption` crate for ownership-safe decoding.
#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
