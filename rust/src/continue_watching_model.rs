//! Read-only projection of unfinished recordings in last-viewed order.
use crate::viewing_history::recordings::{Entry, Source};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use std::pin::Pin;

const KEY: i32 = 256;
const NAME: i32 = 257;
const CHANNEL: i32 = 258;
const PROGRESS: i32 = 259;
const POSITION: i32 = 260;
const DURATION: i32 = 261;
const REMAINING: i32 = 262;
const KIND: i32 = 263;
const RECORDED_ID: i32 = 264;
const VIDEO_ID: i32 = 265;
const THUMBNAIL: i32 = 266;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("QtCore/QAbstractListModel");
        type QAbstractListModel;
        include!("channel_model_helpers.h");
        #[cxx_name = "notifyModelData"]
        fn notify_continue_data(model: Pin<&mut ContinueWatchingModel>);
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("continue_watching_model_helpers.h");
        #[rust_name = "make_continue_watching_model"]
        fn makeContinueWatchingModel() -> UniquePtr<ContinueWatchingModel>;
    }
    #[cfg(feature = "native_tests")]
    unsafe extern "C++" {
        include!("channel_model_test.h");
        #[cxx_name = "checkChannelModel"]
        fn check_continue_model(model: Pin<&mut ContinueWatchingModel>);
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(i32, count, READ = count, NOTIFY = changed)]
        #[qproperty(u32, revision, READ, NOTIFY = changed)]
        type ContinueWatchingModel = super::ContinuePresentation;
        fn count(self: &ContinueWatchingModel) -> i32;
        #[cfg(feature = "qml_tests")]
        #[qinvokable]
        fn load_test(self: Pin<&mut ContinueWatchingModel>, json: QString) -> bool;
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &ContinueWatchingModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn data(self: &ContinueWatchingModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &ContinueWatchingModel) -> QHash_i32_QByteArray;
        #[qsignal]
        fn changed(self: Pin<&mut ContinueWatchingModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut ContinueWatchingModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut ContinueWatchingModel>);
        #[inherit]
        #[cxx_name = "index"]
        fn model_index(
            self: &ContinueWatchingModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
    }
}

#[derive(Default)]
pub struct ContinuePresentation {
    rows: Vec<Row>,
    revision: u32,
}
#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "qml_tests", derive(serde::Deserialize))]
#[cfg_attr(feature = "qml_tests", serde(rename_all = "camelCase"))]
struct Row {
    #[cfg_attr(feature = "qml_tests", serde(default))]
    thumbnail: String,
    recording_key: String,
    program_name: String,
    channel_name: String,
    progress: f64,
    position_ms: f64,
    duration_ms: f64,
    remaining_ms: f64,
    source_kind: String,
    recorded_id: String,
    video_file_id: String,
}
impl From<(Entry, String)> for Row {
    fn from((entry, thumbnail): (Entry, String)) -> Self {
        let (recorded_id, video_file_id) = match entry.identity.source {
            Source::Epgstation {
                recorded_id,
                video_file_id,
                ..
            } => (recorded_id.to_string(), video_file_id.to_string()),
            Source::Local { .. } | Source::Http { .. } => (String::new(), String::new()),
        };
        Self {
            thumbnail,
            recording_key: entry.key,
            program_name: entry.identity.title,
            channel_name: entry.identity.channel,
            progress: entry.position.progress(),
            position_ms: entry.position.position_ms(),
            duration_ms: entry.position.duration_ms(),
            remaining_ms: entry.position.remaining_ms(),
            source_kind: entry.identity.source.kind().into(),
            recorded_id,
            video_file_id,
        }
    }
}
impl ffi::ContinueWatchingModel {
    pub fn count(&self) -> i32 {
        i32::try_from(self.rust().rows.len()).expect("bounded recording page")
    }
    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() { 0 } else { self.count() }
    }
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::default();
        for (role, name) in [
            (KEY, "recordingKey"),
            (NAME, "programName"),
            (CHANNEL, "channelName"),
            (PROGRESS, "progress"),
            (POSITION, "positionMs"),
            (DURATION, "durationMs"),
            (REMAINING, "remainingMs"),
            (KIND, "sourceKind"),
            (RECORDED_ID, "recordedId"),
            (VIDEO_ID, "videoFileId"),
            (THUMBNAIL, "thumbnail"),
        ] {
            roles.insert(role, QByteArray::from(name));
        }
        roles
    }
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        if !index.is_valid()
            || index.column() != 0
            || *index != self.model_index(index.row(), 0, &QModelIndex::default())
        {
            return QVariant::default();
        }
        let Some(row) = usize::try_from(index.row())
            .ok()
            .and_then(|i| self.rust().rows.get(i))
        else {
            return QVariant::default();
        };
        let text = match role {
            THUMBNAIL => &row.thumbnail,
            KEY => &row.recording_key,
            NAME => &row.program_name,
            CHANNEL => &row.channel_name,
            PROGRESS => return QVariant::from(&row.progress),
            POSITION => return QVariant::from(&row.position_ms),
            DURATION => return QVariant::from(&row.duration_ms),
            REMAINING => return QVariant::from(&row.remaining_ms),
            KIND => &row.source_kind,
            RECORDED_ID => &row.recorded_id,
            VIDEO_ID => &row.video_file_id,
            _ => return QVariant::default(),
        };
        QVariant::from(&QString::from(text.as_str()))
    }
    pub(crate) fn replace(self: Pin<&mut Self>, entries: Vec<(Entry, String)>) {
        self.replace_rows(entries.into_iter().map(Row::from).collect());
    }
    #[cfg(feature = "qml_tests")]
    pub fn load_test(self: Pin<&mut Self>, json: QString) -> bool {
        match crate::json::from_slice::<Vec<Row>>(json.to_string().as_bytes()) {
            Ok(rows) => {
                self.replace_rows(rows);
                true
            }
            Err(error) => {
                tracing::error!(
                    error = &error as &dyn std::error::Error,
                    "Load continue model test fixture failed"
                );
                false
            }
        }
    }
    fn replace_rows(mut self: Pin<&mut Self>, rows: Vec<Row>) {
        if self.rust().rows == rows {
            return;
        }
        if self
            .rust()
            .rows
            .iter()
            .map(|row| &row.recording_key)
            .eq(rows.iter().map(|row| &row.recording_key))
        {
            self.as_mut().rust_mut().rows = rows;
            let revision = self.rust().revision.wrapping_add(1);
            self.as_mut().rust_mut().revision = revision;
            ffi::notify_continue_data(self.as_mut());
            self.changed();
            return;
        }
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().rows = rows;
        let revision = self.rust().revision.wrapping_add(1);
        self.as_mut().rust_mut().revision = revision;
        self.as_mut().end_reset_model();
        self.changed();
    }
}
