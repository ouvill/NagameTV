//! Read-only Qt projection of the active video subtitle tracks.
use crate::media_subtitles::Track;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use std::{pin::Pin, sync::Arc};

const ID: i32 = 256;
const NAME: i32 = 257;
const SELECTED: i32 = 258;

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
        include!("subtitle_model_helpers.h");
        #[rust_name = "make_subtitle_model"]
        fn makeSubtitleModel() -> UniquePtr<SubtitleModel>;
    }
    #[cfg(feature = "native_tests")]
    unsafe extern "C++" {
        include!("channel_model_test.h");
        #[cxx_name = "checkChannelModel"]
        fn check_subtitle_model(model: Pin<&mut SubtitleModel>);
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(i32, count, READ = count, NOTIFY = changed)]
        type SubtitleModel = super::SubtitlePresentation;
        fn count(self: &SubtitleModel) -> i32;
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &SubtitleModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn data(self: &SubtitleModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &SubtitleModel) -> QHash_i32_QByteArray;
        #[qsignal]
        fn changed(self: Pin<&mut SubtitleModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut SubtitleModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut SubtitleModel>);
        #[inherit]
        #[cxx_name = "index"]
        fn model_index(
            self: &SubtitleModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
    }
}

#[derive(Default)]
pub struct SubtitlePresentation {
    rows: Arc<[Track]>,
}
impl ffi::SubtitleModel {
    pub fn count(&self) -> i32 {
        i32::try_from(self.rust().rows.len()).expect("bounded subtitle catalogue")
    }
    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() { 0 } else { self.count() }
    }
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::default();
        for (role, name) in [
            (ID, "trackId"),
            (NAME, "displayName"),
            (SELECTED, "trackSelected"),
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
            ID => row.id.clone(),
            NAME => {
                if !row.title.is_empty()
                    && self
                        .rust()
                        .rows
                        .iter()
                        .filter(|track| track.title == row.title)
                        .count()
                        > 1
                {
                    format!("{} · {}", row.title, index.row() + 1)
                } else {
                    row.title.clone()
                }
            }
            SELECTED => return QVariant::from(&row.selected),
            _ => return QVariant::default(),
        };
        QVariant::from(&QString::from(text))
    }
    pub(crate) fn replace(mut self: Pin<&mut Self>, rows: Arc<[Track]>) {
        if self.rust().rows == rows {
            return;
        }
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().rows = rows;
        self.as_mut().end_reset_model();
        self.changed();
    }
}
