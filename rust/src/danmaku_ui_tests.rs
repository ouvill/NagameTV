// Built only by the qml_tests development feature; tests use the real Rust types.
#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("nagametv/src/player.cxxqt.h");
        type Player = crate::player::ffi::Player;
        include!("nagametv/src/video_file_model.cxxqt.h");
        type VideoFileModel = crate::video_file_model::ffi::VideoFileModel;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("danmaku_test.h");
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
        #[qproperty(*mut Player, backend, READ = backend, CONSTANT)]
        type TestRecordingLibrary = super::RecordingLibraryFixture;
        fn backend(self: &TestRecordingLibrary) -> *mut Player;
        #[qinvokable]
        fn load(self: Pin<&mut TestRecordingLibrary>) -> bool;

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

pub struct RecordingLibraryFixture {
    player: cxx::UniquePtr<crate::player::ffi::Player>,
    mock: wiremock::MockServer,
}

impl Default for RecordingLibraryFixture {
    fn default() -> Self {
        use serde_json::json;
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("loopback fixture");
        let mock = runtime.block_on(async {
            let mock = MockServer::builder().listener(listener).start().await;
            let rows = [
                (1, "<b>First recording</b>"),
                (2, "Second recording"),
                (3, "Third recording"),
            ]
            .map(|(id, name)| {
                json!({
                    "id": id, "name": name, "channelName": "Test TV",
                    "startAt": 1700000000000_i64, "endAt": 1700000060000_i64,
                    "isRecording": false, "isProtected": false, "videoFiles": [],
                })
            });
            Mock::given(method("GET"))
                .and(path("/api/channels"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
                .mount(&mock)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/recorded"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(json!({"records":rows,"total":3})),
                )
                .mount(&mock)
                .await;
            for (index, row) in rows.into_iter().enumerate() {
                let route = format!("/api/recorded/{}", index + 1);
                Mock::given(method("GET"))
                    .and(path(route.clone()))
                    .respond_with(ResponseTemplate::new(200).set_body_json(row))
                    .mount(&mock)
                    .await;
                let response = if index == 2 {
                    ResponseTemplate::new(500)
                } else {
                    ResponseTemplate::new(200).set_body_json(json!({"code":200}))
                };
                Mock::given(method("DELETE"))
                    .and(path(route))
                    .respond_with(response)
                    .mount(&mock)
                    .await;
            }
            mock
        });
        crate::features::PLAN.get_or_init(|| {
            crate::features::LaunchPlan::Restricted("recording-delete".parse().expect("test plan"))
        });
        Self {
            player: crate::player::ffi::new_player(),
            mock,
        }
    }
}

impl ffi::TestRecordingLibrary {
    pub fn backend(&self) -> *mut crate::player::ffi::Player {
        use cxx_qt::CxxQtType;
        self.rust().player.as_ptr().cast_mut()
    }

    pub fn load(mut self: std::pin::Pin<&mut Self>) -> bool {
        use cxx_qt::CxxQtType;
        use cxx_qt_lib::QString;
        let state = self.as_mut().rust_mut().get_mut();
        state
            .player
            .pin_mut()
            .browse_epgstation(QString::from(state.mock.uri()), QString::default())
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
    let path = cxx_qt_lib::QString::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/qml"));
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
                    start_ms: None,
                },
                Video {
                    id: u64::MAX,
                    name: "<b>HEVC</b>".into(),
                    filename: "日本語.mkv".into(),
                    kind: VideoType::Encoded,
                    start_ms: None,
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
