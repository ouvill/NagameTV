// Built only by the qml_tests development feature; tests use the real Rust types.
#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("nagametv/src/video_file_model.cxxqt.h");
        type VideoFileModel = crate::video_file_model::ffi::VideoFileModel;
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
        #[qobject]
        #[qml_element]
        #[qproperty(*mut VideoFileModel, files, READ = files, CONSTANT)]
        type TestRecordingFiles = super::RecordingFilesFixture;
        fn files(self: &TestRecordingFiles) -> *mut VideoFileModel;
        #[qinvokable]
        fn populate(self: Pin<&mut TestRecordingFiles>);
        #[qinvokable]
        fn clear(self: Pin<&mut TestRecordingFiles>);

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

pub struct RecordingFilesFixture {
    model: cxx::UniquePtr<crate::video_file_model::ffi::VideoFileModel>,
}
impl Default for RecordingFilesFixture {
    fn default() -> Self {
        Self {
            model: crate::video_file_model::ffi::make_video_file_model(),
        }
    }
}
impl ffi::TestRecordingFiles {
    pub fn files(&self) -> *mut crate::video_file_model::ffi::VideoFileModel {
        use cxx_qt::CxxQtType;
        self.rust().model.as_ptr().cast_mut()
    }
    pub fn populate(mut self: std::pin::Pin<&mut Self>) {
        use crate::epgstation::{Video, VideoType};
        use cxx_qt::CxxQtType;
        self.as_mut().rust_mut().model.pin_mut().replace(
            vec![
                Video {
                    id: 123,
                    name: String::new(),
                    filename: "original.ts".into(),
                    kind: VideoType::Ts,
                },
                Video {
                    id: u64::MAX,
                    name: "<b>HEVC</b>".into(),
                    filename: "日本語.mkv".into(),
                    kind: VideoType::Encoded,
                },
            ]
            .into(),
        );
    }
    pub fn clear(mut self: std::pin::Pin<&mut Self>) {
        use cxx_qt::CxxQtType;
        self.as_mut()
            .rust_mut()
            .model
            .pin_mut()
            .replace(Default::default());
    }
}
