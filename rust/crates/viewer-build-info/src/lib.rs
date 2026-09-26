//! Source identity captured at build time; no runtime Git or environment access.
//! Keep this a normal application dependency so refreshing it never reruns the
//! application's Qt/C++ build script.

include!(concat!(env!("OUT_DIR"), "/build_info.rs"));
