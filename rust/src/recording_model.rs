//! Read-only Qt projection of the EPGStation catalogue.
use crate::epgstation::{Availability, Recording};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use std::{pin::Pin, sync::Arc};

const ID: i32 = 256;
const NAME: i32 = 257;
const CHANNEL: i32 = 258;
const START: i32 = 259;
const END: i32 = 260;
const DESCRIPTION: i32 = 261;
const PLAYABLE: i32 = 262;
const REASON: i32 = 263;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("QtCore/QAbstractListModel");
        type QAbstractListModel;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("recording_model_helpers.h");
        #[rust_name = "make_recording_model"]
        fn makeRecordingModel() -> UniquePtr<RecordingModel>;
    }
    #[cfg(feature = "native_tests")]
    unsafe extern "C++" {
        include!("channel_model_test.h");
        #[cxx_name = "checkChannelModel"]
        fn check_model(model: Pin<&mut RecordingModel>);
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(i32, count, READ = count, NOTIFY = changed)]
        type RecordingModel = super::Presentation;
        fn count(self: &RecordingModel) -> i32;
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &RecordingModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn data(self: &RecordingModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &RecordingModel) -> QHash_i32_QByteArray;
        #[qsignal]
        fn changed(self: Pin<&mut RecordingModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut RecordingModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut RecordingModel>);
        #[inherit]
        #[cxx_name = "index"]
        fn model_index(
            self: &RecordingModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
    }
}

#[derive(Default)]
pub struct Presentation {
    rows: Arc<[Recording]>,
}
impl ffi::RecordingModel {
    pub fn count(&self) -> i32 {
        i32::try_from(self.rust().rows.len()).expect("bounded recording page")
    }
    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() { 0 } else { self.count() }
    }
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::default();
        for (role, name) in [
            (ID, "recordedId"),
            (NAME, "programName"),
            (CHANNEL, "channelName"),
            (START, "startMs"),
            (END, "endMs"),
            (DESCRIPTION, "description"),
            (PLAYABLE, "playable"),
            (REASON, "unavailableReason"),
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
            ID => row.id.to_string(),
            NAME => row.name.clone(),
            CHANNEL => row.channel.clone(),
            DESCRIPTION => row.description.clone(),
            START => return QVariant::from(&(row.start_ms as f64)),
            END => return QVariant::from(&(row.end_ms as f64)),
            PLAYABLE => {
                return QVariant::from(&matches!(row.availability, Availability::Recorded { .. }));
            }
            REASON => match row.availability {
                Availability::Recorded { .. } => "",
                Availability::Recording => "recording",
                Availability::NoTsFile => "no-ts",
            }
            .into(),
            _ => return QVariant::default(),
        };
        QVariant::from(&QString::from(text))
    }
    pub(crate) fn replace(mut self: Pin<&mut Self>, rows: Arc<[Recording]>) {
        if self.rust().rows == rows {
            return;
        }
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().rows = rows;
        self.as_mut().end_reset_model();
        self.changed();
    }
}
