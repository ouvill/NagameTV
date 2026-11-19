//! Read-only view of verified LAN endpoints.
use crate::services::discovery::Server;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use std::pin::Pin;

const URL: i32 = 256;
const NAME: i32 = 257;
const VERSION: i32 = 258;

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
        include!("lan_discovery_model_helpers.h");
        #[rust_name = "make_lan_discovery_model"]
        fn makeLanDiscoveryModel() -> UniquePtr<LanDiscoveryModel>;
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        type LanDiscoveryModel = super::LanPresentation;
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &LanDiscoveryModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn data(self: &LanDiscoveryModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &LanDiscoveryModel) -> QHash_i32_QByteArray;
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut LanDiscoveryModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut LanDiscoveryModel>);
        #[inherit]
        #[cxx_name = "index"]
        fn model_index(
            self: &LanDiscoveryModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
    }
}

#[derive(Default)]
pub struct LanPresentation {
    rows: Vec<Server>,
}

impl ffi::LanDiscoveryModel {
    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() {
            0
        } else {
            i32::try_from(self.rust().rows.len()).expect("bounded LAN search")
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::default();
        for (role, name) in [
            (URL, "serverUrl"),
            (NAME, "productName"),
            (VERSION, "serverVersion"),
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
        let Some(row) = self.rust().rows.get(index.row() as usize) else {
            return QVariant::default();
        };
        let value = match role {
            URL => &row.url,
            NAME => row.name,
            VERSION => row.version.as_deref().unwrap_or_default(),
            _ => return QVariant::default(),
        };
        QVariant::from(&QString::from(value))
    }

    pub fn replace(mut self: Pin<&mut Self>, rows: Vec<Server>) {
        if rows != self.rust().rows {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().rows = rows;
            self.end_reset_model();
        }
    }
}
