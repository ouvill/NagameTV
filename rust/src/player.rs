#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("qt_helpers.h");
        type QQuickItem;
        #[cxx_name = "configureQtQuickOpenGl"]
        fn configure_qt_quick_open_gl();
        #[cxx_name = "qQuickItemAddress"]
        unsafe fn q_quick_item_address(item: *mut QQuickItem) -> usize;
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, server, READ, NOTIFY)]
        #[qproperty(QString, status, READ, NOTIFY)]
        #[qproperty(QStringList, channels, READ, NOTIFY)]
        #[qproperty(i32, selected, READ, NOTIFY)]
        #[qproperty(bool, loading, READ, NOTIFY)]
        #[qproperty(bool, subtitles_enabled, READ, NOTIFY)]
        #[qproperty(bool, epg_enabled, READ, NOTIFY)]
        #[qproperty(bool, subtitles_allowed, READ, NOTIFY)]
        #[qproperty(bool, epg_allowed, READ, NOTIFY)]
        #[qproperty(bool, subtitles_active, READ, NOTIFY)]
        #[qproperty(bool, subtitle_display, READ, NOTIFY)]
        #[qproperty(QString, subtitle_data, READ, NOTIFY)]
        #[qproperty(QString, subtitle_status, READ, NOTIFY)]
        #[qproperty(QString, epg_data, READ, NOTIFY)]
        #[qproperty(QString, epg_status, READ, NOTIFY)]
        #[qproperty(QString, diagnostics, READ, NOTIFY)]
        type Player = super::PlayerRust;
        #[qinvokable]
        fn configure_features(self: Pin<&mut Player>, subtitles: bool, epg: bool);
        #[qinvokable]
        fn display_subtitles(self: Pin<&mut Player>, display: bool);
        #[qinvokable]
        fn poll_subtitles(self: Pin<&mut Player>);
        #[qinvokable]
        fn guide_open(self: Pin<&mut Player>, open: bool);
        #[qinvokable]
        fn refresh_epg(self: Pin<&mut Player>);
        #[qinvokable]
        unsafe fn attach(self: Pin<&mut Player>, item: *mut QQuickItem) -> bool;
        #[qinvokable]
        fn connect_server(self: Pin<&mut Player>, server: QString);
        #[qinvokable]
        fn select(self: Pin<&mut Player>, index: i32);
        #[qinvokable]
        fn play(self: Pin<&mut Player>);
        #[qinvokable]
        fn stop(self: Pin<&mut Player>);
        #[qinvokable]
        fn volume(self: Pin<&mut Player>, value: f64);
        #[qinvokable]
        fn poll(self: Pin<&mut Player>);
        #[qinvokable]
        fn shutdown(self: Pin<&mut Player>);
    }
}

use crate::{
    features::{self, program_info::ProgramInfo, subtitles},
    playback, services,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use std::pin::Pin;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct PlayerRust {
    server: QString,
    status: QString,
    channels: QStringList,
    selected: i32,
    loading: bool,
    subtitles_enabled: bool,
    epg_enabled: bool,
    subtitles_allowed: bool,
    epg_allowed: bool,
    subtitles_active: bool,
    subtitle_display: bool,
    subtitle_data: QString,
    subtitle_status: QString,
    epg_data: QString,
    epg_status: QString,
    diagnostics: QString,
    subtitle_session: Option<subtitles::Session>,
    epg: ProgramInfo,
    active_service: Option<u64>,
    guide_visible: bool,
    guide_revision: u64,
    guide_service: Option<u64>,
    next_guide: Instant,
    next_diagnostic: Instant,
    request: Option<services::Request>,
    network: Option<services::Network>,
    playback: Option<playback::Playback>,
    entries: Vec<services::Service>,
}

impl Default for PlayerRust {
    fn default() -> Self {
        let plan = *features::PLAN
            .get()
            .expect("LaunchPlan initialized before Qt");
        let network = services::Network::new();
        let status = network
            .as_ref()
            .err()
            .cloned()
            .unwrap_or_else(|| "サーバーに接続してください".into());
        Self {
            server: QString::from(std::env::var("MIRAKURUN_SERVER").unwrap_or_default()),
            status: QString::from(status),
            channels: QStringList::default(),
            selected: -1,
            loading: false,
            subtitles_enabled: plan.subtitles,
            epg_enabled: plan.epg,
            subtitles_allowed: !plan.locked || plan.subtitles,
            epg_allowed: !plan.locked || plan.epg,
            subtitles_active: false,
            subtitle_display: true,
            subtitle_data: QString::default(),
            subtitle_status: QString::from("停止中"),
            epg_data: QString::from("[]"),
            epg_status: QString::from("無効"),
            diagnostics: QString::default(),
            subtitle_session: None,
            epg: ProgramInfo::default(),
            active_service: None,
            guide_visible: false,
            guide_revision: 0,
            guide_service: None,
            next_guide: Instant::now(),
            next_diagnostic: Instant::now(),
            request: None,
            network: network.ok(),
            playback: playback::take_preloaded(),
            entries: vec![],
        }
    }
}

macro_rules! property_setter {
    ($method:ident, $field:ident, $signal:ident, $ty:ty) => {
        fn $method(mut self: Pin<&mut Self>, value: $ty) {
            if self.rust().$field != value {
                self.as_mut().rust_mut().$field = value;
                self.as_mut().$signal();
            }
        }
    };
}

impl ffi::Player {
    property_setter!(set_server, server, server_changed, QString);
    property_setter!(set_status, status, status_changed, QString);
    property_setter!(set_channels, channels, channels_changed, QStringList);
    property_setter!(set_selected, selected, selected_changed, i32);
    property_setter!(set_loading, loading, loading_changed, bool);
    property_setter!(
        set_subtitles_enabled,
        subtitles_enabled,
        subtitles_enabled_changed,
        bool
    );
    property_setter!(set_epg_enabled, epg_enabled, epg_enabled_changed, bool);
    property_setter!(
        set_subtitles_active,
        subtitles_active,
        subtitles_active_changed,
        bool
    );
    property_setter!(
        set_subtitle_display,
        subtitle_display,
        subtitle_display_changed,
        bool
    );
    property_setter!(
        set_subtitle_data,
        subtitle_data,
        subtitle_data_changed,
        QString
    );
    property_setter!(
        set_subtitle_status,
        subtitle_status,
        subtitle_status_changed,
        QString
    );
    property_setter!(set_epg_data, epg_data, epg_data_changed, QString);
    property_setter!(set_epg_status, epg_status, epg_status_changed, QString);
    property_setter!(set_diagnostics, diagnostics, diagnostics_changed, QString);

    /// READY joins streaming callbacks before dropping their subscriptions/state.
    fn end_stream(mut self: Pin<&mut Self>) -> Result<(), String> {
        if let Some(playback) = &self.rust().playback {
            playback.stop().map_err(|e| e.to_string())?;
        }
        self.as_mut().rust_mut().subtitle_session = None;
        self.as_mut().rust_mut().active_service = None;
        self.as_mut().set_subtitles_active(false);
        self.as_mut().set_subtitle_data(QString::default());
        self.as_mut().set_subtitle_status(QString::from("停止中"));
        Ok(())
    }
    fn configure_epg(mut self: Pin<&mut Self>) {
        let server = if self.rust().epg_enabled && !self.rust().entries.is_empty() {
            Some(self.server().to_string())
        } else {
            None
        };
        self.as_mut().rust_mut().epg.configure(server);
        if !self.rust().epg_enabled {
            self.as_mut().guide_open(false);
        }
    }
    pub fn configure_features(mut self: Pin<&mut Self>, subtitles: bool, epg: bool) {
        let subtitles = subtitles && self.rust().subtitles_allowed;
        let epg = epg && self.rust().epg_allowed;
        let restart =
            subtitles != self.rust().subtitles_enabled && self.rust().active_service.is_some();
        if restart {
            if let Err(error) = self.as_mut().end_stream() {
                self.status_text(error);
                return;
            }
        }
        self.as_mut().set_subtitles_enabled(subtitles);
        self.as_mut().set_epg_enabled(epg);
        self.as_mut().configure_epg();
        if restart {
            self.as_mut().play();
        }
    }
    pub fn display_subtitles(mut self: Pin<&mut Self>, display: bool) {
        self.as_mut().set_subtitle_display(display);
        self.set_subtitle_data(QString::default());
    }
    pub fn poll_subtitles(self: Pin<&mut Self>) {
        let update = self.rust().subtitle_session.as_ref().map(|session| {
            session.poll(
                self.rust()
                    .playback
                    .as_ref()
                    .and_then(playback::Playback::position),
            )
        });
        match update {
            Some(subtitles::SubtitleUpdate::Show(cue)) if self.rust().subtitle_display => {
                self.set_subtitle_data(QString::from(
                    serde_json::to_string(&cue).unwrap_or_default(),
                ));
            }
            Some(subtitles::SubtitleUpdate::Clear) => self.set_subtitle_data(QString::default()),
            _ => {}
        }
    }
    pub fn guide_open(mut self: Pin<&mut Self>, open: bool) {
        self.as_mut().rust_mut().guide_visible = open && self.rust().epg_enabled;
        self.as_mut().rust_mut().next_guide = Instant::now();
        if !self.rust().guide_visible {
            self.set_epg_data(QString::from("[]"));
        }
    }
    pub fn refresh_epg(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().epg.refresh();
    }
    fn poll_features(mut self: Pin<&mut Self>) {
        {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            if let Some(network) = &this.network {
                this.epg.poll(network);
            }
        }
        let status = self.rust().epg.status.clone();
        self.as_mut().set_epg_status(QString::from(status));
        let service = self
            .rust()
            .entries
            .get(self.rust().selected as usize)
            .map(|s| s.id);
        if self.rust().guide_visible
            && (self.rust().guide_revision != self.rust().epg.revision
                || self.rust().guide_service != service
                || Instant::now() >= self.rust().next_guide)
        {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let data = self.rust().epg.view(service, now);
            let revision = self.rust().epg.revision;
            self.as_mut().rust_mut().guide_revision = revision;
            self.as_mut().rust_mut().guide_service = service;
            self.as_mut().rust_mut().next_guide = Instant::now() + Duration::from_secs(30);
            self.as_mut().set_epg_data(QString::from(data));
        }
        if Instant::now() >= self.rust().next_diagnostic {
            let (subscriptions, pending, decoded) = self
                .rust()
                .subtitle_session
                .as_ref()
                .map(|s| s.counters())
                .unwrap_or_default();
            let (tasks, programs, stopping) = self.rust().epg.counters();
            let rss = std::fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find(|l| l.starts_with("VmRSS:"))
                        .map(str::to_owned)
                })
                .unwrap_or_default();
            let text = format!(
                "{rss} | 字幕: 購読 {subscriptions}, 待機 {pending}, 受信 {decoded} | EPG: タスク {tasks}, 番組 {programs}, 停止待ち {stopping}"
            );
            eprintln!("METRICS {text}");
            self.as_mut().set_diagnostics(QString::from(text));
            self.as_mut().rust_mut().next_diagnostic = Instant::now() + Duration::from_secs(10);
        }
    }
    fn status_text(mut self: Pin<&mut Self>, text: impl Into<String>) {
        self.as_mut().set_status(QString::from(text.into()));
    }
    /// QML supplies a live GUI-thread item and calls shutdown before destroying it.
    pub unsafe fn attach(mut self: Pin<&mut Self>, item: *mut ffi::QQuickItem) -> bool {
        let address = unsafe { ffi::q_quick_item_address(item) };
        let result = self
            .as_mut()
            .rust_mut()
            .playback
            .as_mut()
            .ok_or_else(|| "Playback unavailable".to_owned())
            .and_then(|p| unsafe { p.attach(address) }.map_err(|e| e.to_string()));
        if let Err(error) = result {
            self.status_text(error);
            return false;
        }
        true
    }
    pub fn connect_server(mut self: Pin<&mut Self>, server: QString) {
        self.as_mut().rust_mut().request = None;
        self.as_mut().set_loading(false);
        self.as_mut().rust_mut().epg.configure(None);
        self.as_mut().set_epg_data(QString::from("[]"));
        if let Err(error) = self.as_mut().end_stream() {
            self.status_text(error);
            return;
        }
        self.as_mut().rust_mut().entries.clear();
        self.as_mut().set_channels(QStringList::default());
        self.as_mut().set_selected(-1);
        let server = match services::server_url(&server.to_string()) {
            Ok(server) => server,
            Err(error) => {
                self.status_text(error);
                return;
            }
        };
        let Some(network) = &self.rust().network else {
            self.status_text("Network unavailable");
            return;
        };
        let request = network.fetch(&server);
        self.as_mut().rust_mut().request = Some(request);
        self.as_mut().set_server(QString::from(server));
        self.as_mut().set_loading(true);
        self.status_text("チャンネルを取得中…");
    }
    pub fn select(mut self: Pin<&mut Self>, index: i32) {
        if index < 0 || index as usize >= self.rust().entries.len() {
            return;
        }
        self.as_mut().set_selected(index);
        self.play();
    }
    pub fn play(mut self: Pin<&mut Self>) {
        let Some(entry) = self.rust().entries.get(*self.selected() as usize) else {
            return;
        };
        let (id, name) = (entry.id, entry.name.clone());
        let server = self.server().to_string();
        if self.rust().active_service == Some(id) {
            return;
        }
        if let Err(error) = self.as_mut().end_stream() {
            self.status_text(error);
            return;
        }
        if self.rust().subtitles_enabled {
            let result = self
                .rust()
                .playback
                .as_ref()
                .ok_or_else(|| "Playback unavailable".to_owned())
                .and_then(|p| subtitles::Session::start(p.element(), id));
            match result {
                Ok(session) => {
                    self.as_mut().rust_mut().subtitle_session = Some(session);
                    self.as_mut().set_subtitles_active(true);
                    self.as_mut().set_subtitle_status(QString::from("解析中"));
                }
                Err(error) => self.as_mut().set_subtitle_status(QString::from(error)),
            }
        }
        let result = self.rust().playback.as_ref().map(|p| p.play(&server, id));
        match result {
            Some(Ok(_)) => {
                self.as_mut().rust_mut().active_service = Some(id);
                self.as_mut().status_text(format!("接続中: {name}"));
            }
            Some(Err(error)) => {
                let text = error.to_string();
                let _ = self.as_mut().end_stream();
                self.as_mut().status_text(text);
            }
            None => self.as_mut().status_text("Playback unavailable"),
        }
    }
    pub fn stop(mut self: Pin<&mut Self>) {
        match self.as_mut().end_stream() {
            Ok(()) => self.status_text("停止"),
            Err(error) => self.status_text(error),
        }
    }
    pub fn volume(self: Pin<&mut Self>, value: f64) {
        if let Some(playback) = &self.rust().playback {
            playback.set_volume(value);
        }
    }
    pub fn poll(mut self: Pin<&mut Self>) {
        let fetched = self
            .rust()
            .request
            .as_ref()
            .and_then(services::Request::poll);
        if let Some(result) = fetched {
            self.as_mut().rust_mut().request = None;
            self.as_mut().set_loading(false);
            match result {
                Ok(entries) => {
                    let mut names = QStringList::default();
                    for entry in &entries {
                        names.append(QString::from(entry.name.clone()));
                    }
                    self.as_mut().rust_mut().entries = entries;
                    self.as_mut().set_channels(names);
                    self.as_mut().set_selected(0);
                    self.as_mut().configure_epg();
                    self.as_mut()
                        .status_text("チャンネルを選んで再生してください");
                }
                Err(error) => self
                    .as_mut()
                    .status_text(format!("チャンネル取得失敗: {error}")),
            }
        }
        self.as_mut().poll_features();
        let result = self.rust().playback.as_ref().map(playback::Playback::poll);
        match result {
            Some(Ok(true)) => {
                if let Some(entry) = self.rust().entries.get(*self.selected() as usize) {
                    let text = format!("再生中: {}", entry.name);
                    eprintln!("Pipeline PLAYING service {}", entry.id);
                    self.as_mut().status_text(text);
                }
            }
            Some(Err(error)) => {
                let text = error.to_string();
                eprintln!("Playback error: {text}");
                self.as_mut().stop();
                self.status_text(format!("再生エラー: {text}"));
            }
            _ => {}
        }
    }
    pub fn shutdown(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().epg.configure(None);
        self.as_mut().guide_open(false);
        let _ = self.as_mut().end_stream();
        self.as_mut().rust_mut().request = None;
        if let Some(playback) = self.as_mut().rust_mut().playback.as_mut() {
            playback.shutdown();
        }
    }
}

impl Drop for PlayerRust {
    fn drop(&mut self) {
        // Qt normally calls shutdown; also cover a failed QML construction.
        if let Some(playback) = self.playback.as_mut() {
            playback.shutdown();
        }
        self.subtitle_session = None;
        self.epg.configure(None);
    }
}
