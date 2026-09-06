use super::ffi;
use crate::runtime::Presentation;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;
use viewer_core::app::{Event, Preference};

impl ffi::Player {
    pub(super) fn dispatch(mut self: Pin<&mut Self>, event: Event) {
        self.as_mut().rust_mut().runtime.dispatch(event);
        self.finish();
    }
    fn finish(mut self: Pin<&mut Self>) -> bool {
        let mut accepted = true;
        let mut comments = Vec::new();
        loop {
            let presentation = self.as_mut().rust_mut().runtime.take_presentation();
            if presentation.is_empty() {
                break;
            }
            for effect in presentation {
                match effect {
                    Presentation::Comment(text) => comments.push(text),
                    Presentation::ApplyLanguage(preference) => {
                        let language = ffi::apply_ui_language(&QString::from(&preference));
                        let result = if language.is_empty() {
                            Err("Could not load UI translation".into())
                        } else {
                            Ok(language.to_string())
                        };
                        accepted &= self
                            .as_mut()
                            .rust_mut()
                            .runtime
                            .request(Event::LanguageApplied { preference, result });
                    }
                }
            }
        }
        self.as_mut().render();
        for text in comments {
            self.as_mut().comment_received(QString::from(text));
        }
        accepted
    }
    pub fn request_volume(self: Pin<&mut Self>, value: f64) {
        self.dispatch(Event::Preference(Preference::Volume(value)));
    }
    pub fn request_audio_muted(self: Pin<&mut Self>, value: bool) {
        self.dispatch(Event::Preference(Preference::Muted(value)));
    }
    pub fn request_danmaku_enabled(self: Pin<&mut Self>, value: bool) {
        self.dispatch(Event::Preference(Preference::Comments(value)));
    }
    pub fn request_comment_font_size(self: Pin<&mut Self>, value: f64) {
        self.dispatch(Event::Preference(Preference::CommentFontSize(value)));
    }
    pub fn request_comment_opacity(self: Pin<&mut Self>, value: f64) {
        self.dispatch(Event::Preference(Preference::CommentOpacity(value)));
    }
    pub fn request_comment_speed(self: Pin<&mut Self>, value: f64) {
        self.dispatch(Event::Preference(Preference::CommentSpeed(value)));
    }
    pub fn request_subtitles_enabled(self: Pin<&mut Self>, value: bool) {
        self.dispatch(Event::Preference(Preference::Subtitles(value)));
    }
    pub fn play(self: Pin<&mut Self>) {
        self.dispatch(Event::Play);
    }
    pub fn stop(self: Pin<&mut Self>) {
        self.dispatch(Event::Stop);
    }
    pub fn save_settings(self: Pin<&mut Self>) {
        self.dispatch(Event::SaveSettings);
    }
    pub fn video_ready(self: Pin<&mut Self>) {
        self.dispatch(Event::VideoReady);
    }
    pub fn select_channel(self: Pin<&mut Self>, index: i32) {
        if let Ok(index) = usize::try_from(index) {
            self.dispatch(Event::SelectChannel(index));
        }
    }
    pub fn change_channel(self: Pin<&mut Self>, offset: i32) {
        self.dispatch(Event::ChangeChannel(offset));
    }
    pub fn refresh_channels(self: Pin<&mut Self>, force: bool) {
        self.dispatch(Event::Refresh { force });
    }
    pub fn select_audio_track(self: Pin<&mut Self>, key: QString) {
        self.dispatch(Event::SelectAudio(key.to_string()));
    }
    pub fn connect_server(mut self: Pin<&mut Self>, server: QString) -> bool {
        let accepted = self
            .as_mut()
            .rust_mut()
            .runtime
            .request(Event::ConnectServer(server.to_string()));
        let applied = self.finish();
        accepted && applied
    }
    pub fn change_language(mut self: Pin<&mut Self>, language: QString) -> bool {
        let accepted = self
            .as_mut()
            .rust_mut()
            .runtime
            .request(Event::ChangeLanguage(language.to_string()));
        let applied = self.finish();
        accepted && applied
    }
    pub fn poll_events(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().runtime.tick();
        self.finish();
    }
    pub fn poll_subtitles(mut self: Pin<&mut Self>) {
        let changed = self.as_mut().rust_mut().runtime.poll_subtitles();
        if changed {
            self.finish();
        }
    }
    /// # Safety
    /// QML supplies a live GUI-thread item and keeps it alive for the sink lifetime.
    pub unsafe fn attach_video_item(mut self: Pin<&mut Self>, item: *mut ffi::QQuickItem) -> bool {
        // SAFETY: the caller guarantees the item lifetime and GUI-thread access.
        let address = unsafe { ffi::q_quick_item_address(item) };
        let attached = self
            .as_mut()
            .rust_mut()
            .runtime
            .attach_video(address as *mut std::ffi::c_void);
        self.finish();
        attached
    }
    pub fn subtitle_glyph_outline(&self, text: QString, font: ffi::QFont) -> QString {
        ffi::subtitle_outline_path(&text, &font)
    }
    pub fn video_stats(&self) -> QString {
        QString::from(self.rust().runtime.video_stats())
    }
    pub fn record_ui_state(
        mut self: Pin<&mut Self>,
        guide: bool,
        channels: bool,
        live_comments: i32,
    ) {
        self.as_mut()
            .rust_mut()
            .runtime
            .record_ui(guide, channels, live_comments);
    }
    pub fn open_log_folder(self: Pin<&mut Self>) -> bool {
        self.rust().runtime.log_directory().is_some_and(|path| {
            ffi::open_playback_log_directory(&QString::from(path.to_string_lossy().as_ref()))
        })
    }
}
