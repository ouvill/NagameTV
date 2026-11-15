//! Read-only Qt projection of discovered Linux HDMI-CEC adapters.
use crate::hdmi_cec::DeviceInfo;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use std::{pin::Pin, sync::Arc};

const PATH: i32 = 256;
const NAME: i32 = 257;
const DRIVER: i32 = 258;

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
        include!("hdmi_cec_model_helpers.h");
        #[rust_name = "make_hdmi_cec_model"]
        fn makeHdmiCecDeviceModel() -> UniquePtr<HdmiCecDeviceModel>;
    }
    #[cfg(feature = "native_tests")]
    unsafe extern "C++" {
        include!("channel_model_test.h");
        #[cxx_name = "checkChannelModel"]
        fn check_hdmi_cec_model(model: Pin<&mut HdmiCecDeviceModel>);
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(i32, count, READ = count, NOTIFY = changed)]
        type HdmiCecDeviceModel = super::HdmiCecPresentation;
        fn count(self: &HdmiCecDeviceModel) -> i32;
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &HdmiCecDeviceModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn data(self: &HdmiCecDeviceModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &HdmiCecDeviceModel) -> QHash_i32_QByteArray;
        #[qsignal]
        fn changed(self: Pin<&mut HdmiCecDeviceModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut HdmiCecDeviceModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut HdmiCecDeviceModel>);
        #[inherit]
        #[cxx_name = "index"]
        fn model_index(
            self: &HdmiCecDeviceModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
    }
}

#[derive(Default)]
pub struct HdmiCecPresentation {
    rows: Arc<[DeviceInfo]>,
}
impl ffi::HdmiCecDeviceModel {
    pub fn count(&self) -> i32 {
        i32::try_from(self.rust().rows.len()).expect("bounded CEC device catalogue")
    }
    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() { 0 } else { self.count() }
    }
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::default();
        for (role, name) in [(PATH, "path"), (NAME, "name"), (DRIVER, "driver")] {
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
        let Ok(position) = usize::try_from(index.row()) else {
            return QVariant::default();
        };
        let Some(row) = self.rust().rows.get(position) else {
            return QVariant::default();
        };
        let text = match role {
            PATH => &row.path,
            NAME => &row.name,
            DRIVER => &row.driver,
            _ => return QVariant::default(),
        };
        QVariant::from(&QString::from(text))
    }
    pub(crate) fn replace(mut self: Pin<&mut Self>, rows: Arc<[DeviceInfo]>) {
        if self.rust().rows == rows {
            return;
        }
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().rows = rows;
        self.as_mut().end_reset_model();
        self.changed();
    }
}
