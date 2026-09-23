//! Read-only Qt projection of the EPGStation catalogue.
use crate::epgstation::{Video, VideoType};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use std::{pin::Pin, sync::Arc};

const ID: i32 = 256;
const NAME: i32 = 257;
const FILENAME: i32 = 258;
const ORIGINAL: i32 = 259;

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
        include!("video_file_model_helpers.h");
        #[rust_name = "make_video_file_model"]
        fn makeVideoFileModel() -> UniquePtr<VideoFileModel>;
    }
    #[cfg(feature = "native_tests")]
    unsafe extern "C++" {
        include!("channel_model_test.h");
        #[cxx_name = "checkChannelModel"]
        fn check_video_model(model: Pin<&mut VideoFileModel>);
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(i32, count, READ = count, NOTIFY = changed)]
        type VideoFileModel = super::VideoFilePresentation;
        fn count(self: &VideoFileModel) -> i32;
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &VideoFileModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn data(self: &VideoFileModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &VideoFileModel) -> QHash_i32_QByteArray;
        #[qsignal]
        fn changed(self: Pin<&mut VideoFileModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut VideoFileModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut VideoFileModel>);
        #[inherit]
        #[cxx_name = "index"]
        fn model_index(
            self: &VideoFileModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
    }
}

#[derive(Default)]
pub struct VideoFilePresentation {
    rows: Arc<[Video]>,
}
impl ffi::VideoFileModel {
    pub fn count(&self) -> i32 {
        i32::try_from(self.rust().rows.len()).expect("bounded recording page")
    }
    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() { 0 } else { self.count() }
    }
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::default();
        for (role, name) in [
            (ID, "videoId"),
            (NAME, "displayName"),
            (FILENAME, "fileName"),
            (ORIGINAL, "originalTs"),
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
            FILENAME => row.filename.clone(),
            ORIGINAL => return QVariant::from(&(row.kind == VideoType::Ts)),
            _ => return QVariant::default(),
        };
        QVariant::from(&QString::from(text))
    }
    pub(crate) fn replace(mut self: Pin<&mut Self>, rows: Arc<[Video]>) {
        if self.rust().rows == rows {
            return;
        }
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().rows = rows;
        self.as_mut().end_reset_model();
        self.changed();
    }
}
