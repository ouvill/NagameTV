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
        type Player = super::PlayerRust;
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

use crate::{playback, services};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use std::pin::Pin;

pub struct PlayerRust {
    server: QString,
    status: QString,
    channels: QStringList,
    selected: i32,
    loading: bool,
    request: Option<services::Request>,
    network: Option<services::Network>,
    playback: Option<playback::Playback>,
    entries: Vec<services::Service>,
}

impl Default for PlayerRust {
    fn default() -> Self {
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
        if let Some(playback) = &self.rust().playback {
            if let Err(error) = playback.stop() {
                self.status_text(error.to_string());
                return;
            }
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
        if let Some(playback) = &self.rust().playback {
            match playback.play(&server, id) {
                Ok(true) => self.as_mut().status_text(format!("接続中: {name}")),
                Ok(false) => {}
                Err(error) => {
                    let text = error.to_string();
                    let _ = playback.stop();
                    self.as_mut().status_text(text);
                }
            }
        }
    }
    pub fn stop(mut self: Pin<&mut Self>) {
        if let Some(playback) = &self.rust().playback {
            match playback.stop() {
                Ok(()) => self.as_mut().status_text("停止"),
                Err(error) => self.as_mut().status_text(error.to_string()),
            }
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
                    self.as_mut()
                        .status_text("チャンネルを選んで再生してください");
                }
                Err(error) => self
                    .as_mut()
                    .status_text(format!("チャンネル取得失敗: {error}")),
            }
        }
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
        self.as_mut().rust_mut().request = None;
        if let Some(playback) = self.as_mut().rust_mut().playback.as_mut() {
            playback.shutdown();
        }
    }
}
