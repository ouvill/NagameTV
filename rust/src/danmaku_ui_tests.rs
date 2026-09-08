// Built only by the qml_tests development feature; tests use the real Rust types.
#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("danmaku_test.h");
        fn run_qml_tests(path: &QString) -> i32;
        #[cfg(feature = "native_tests")]
        fn run_qml_test_args(arguments: &[String]) -> i32;
    }
}

#[cfg(feature = "native_tests")]
pub fn run_with_arguments(arguments: &[String]) -> i32 {
    cxx_qt::init_qml_module!("MinimalViewer");
    ffi::run_qml_test_args(arguments)
}

pub fn run() -> i32 {
    cxx_qt::init_qml_module!("MinimalViewer");
    let path = cxx_qt_lib::QString::from(concat!(env!("CARGO_MANIFEST_DIR"), "/qml/tests"));
    ffi::run_qml_tests(&path)
}
