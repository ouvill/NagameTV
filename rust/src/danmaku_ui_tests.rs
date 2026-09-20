// Built only by the qml_tests development feature; tests use the real Rust types.
#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("nagametv/src/danmaku_test.h");
        fn run_qml_tests(path: &QString) -> i32;
        #[rust_name = "send_input_method"]
        fn sendTestInputMethod(preedit: &QString, commit: &QString) -> bool;
        #[rust_name = "send_input_method_cursor"]
        fn sendTestInputMethodCursor(preedit: &QString) -> bool;
        #[rust_name = "send_forwarded_key"]
        fn sendTestForwardedKey(key: i32, modifiers: i32, text: &QString, repeat: bool) -> bool;
        #[cfg(feature = "native_tests")]
        fn run_qml_test_args(arguments: &[String]) -> i32;
    }
    unsafe extern "RustQt" {
        // Inspect compile-time features without constructing Player, whose
        // startup plan and persistent resources belong to the native tests.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, evaluation_comment_list, READ = evaluation_comment_list, CONSTANT)]
        type TestBuildConfiguration = super::BuildConfiguration;
        fn evaluation_comment_list(self: &TestBuildConfiguration) -> bool;

        #[qobject]
        #[qml_element]
        type TestInputMethod = super::InputMethod;
        #[qinvokable]
        fn compose(self: &TestInputMethod, preedit: &QString, commit: &QString) -> bool;
        #[qinvokable]
        fn cursor_preedit(self: &TestInputMethod, preedit: &QString) -> bool;
        #[qinvokable]
        fn forward_key(
            self: &TestInputMethod,
            key: i32,
            modifiers: i32,
            text: &QString,
            repeat: bool,
        ) -> bool;
    }
}

#[derive(Default)]
pub struct BuildConfiguration;
impl ffi::TestBuildConfiguration {
    pub fn evaluation_comment_list(&self) -> bool {
        cfg!(feature = "evaluation-comment-list")
    }
}

#[derive(Default)]
pub struct InputMethod;
impl ffi::TestInputMethod {
    pub fn compose(&self, preedit: &cxx_qt_lib::QString, commit: &cxx_qt_lib::QString) -> bool {
        ffi::send_input_method(preedit, commit)
    }

    pub fn cursor_preedit(&self, preedit: &cxx_qt_lib::QString) -> bool {
        ffi::send_input_method_cursor(preedit)
    }

    pub fn forward_key(
        &self,
        key: i32,
        modifiers: i32,
        text: &cxx_qt_lib::QString,
        repeat: bool,
    ) -> bool {
        ffi::send_forwarded_key(key, modifiers, text, repeat)
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
