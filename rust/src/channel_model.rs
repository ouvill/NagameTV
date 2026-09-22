//! Qt presentation of an immutable domain catalog; filtering never selects a station.
use crate::channels::{Band, Channel};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QList, QModelIndex, QString, QVariant,
};
use std::{pin::Pin, sync::Arc};

#[cfg(feature = "native_tests")]
pub mod checks;

const INDEX: i32 = 256;
const LABEL: i32 = 257;
const BAND: i32 = 258;
const LOGO: i32 = 259;
const SERVICE: i32 = 260;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("QtCore/QAbstractListModel");
        type QAbstractListModel;
        include!("channel_model_types.h");
        type ChannelFilterProxy;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("channel_model_helpers.h");
        #[rust_name = "make_channel_model"]
        fn makeChannelModel() -> UniquePtr<ChannelModel>;
        #[cxx_name = "channelRow"]
        fn channel_row(model: &ChannelModel, row: i32) -> QVariant;
        #[cxx_name = "channelRow"]
        fn filtered_row(model: &ChannelFilterModel, row: i32) -> QVariant;
        #[cxx_name = "channelCount"]
        fn filtered_count(model: &ChannelFilterModel) -> i32;
        #[cxx_name = "channelSourceData"]
        fn source_data(model: &ChannelFilterModel, row: i32, role: i32) -> QVariant;
        #[cxx_name = "channelRowForIndex"]
        fn source_row_for_index(model: &ChannelModel, wanted: i32, role: i32) -> i32;
        #[cxx_name = "channelRowForIndex"]
        fn row_for_index(model: &ChannelFilterModel, wanted: i32, role: i32) -> i32;
        #[cxx_name = "connectChannelChanges"]
        fn connect_changes(model: Pin<&mut ChannelFilterModel>);
    }
    #[cfg(feature = "native_tests")]
    unsafe extern "C++" {
        include!("channel_model_test.h");
        #[rust_name = "make_channel_filter_model"]
        fn makeChannelFilterModel() -> UniquePtr<ChannelFilterModel>;
        #[cxx_name = "setChannelTestSource"]
        fn set_test_source(filter: Pin<&mut ChannelFilterModel>, source: Pin<&mut ChannelModel>);
        #[cxx_name = "checkChannelModel"]
        fn check_source_model(model: Pin<&mut ChannelModel>);
        #[cxx_name = "checkChannelModel"]
        fn check_filter_model(model: Pin<&mut ChannelFilterModel>);
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(i32, count, READ = count, NOTIFY = changed)]
        #[qproperty(u32, revision, READ, NOTIFY = changed)]
        type ChannelModel = super::ChannelPresentation;
        fn count(self: &ChannelModel) -> i32;
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &ChannelModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn data(self: &ChannelModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &ChannelModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        fn row(self: &ChannelModel, index: i32) -> QVariant;
        #[qinvokable]
        fn has_band(self: &ChannelModel, band: QString) -> bool;
        #[qinvokable]
        fn row_for_channel(self: &ChannelModel, index: i32) -> i32;
        #[qsignal]
        fn changed(self: Pin<&mut ChannelModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut ChannelModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut ChannelModel>);
        #[inherit]
        #[cxx_name = "index"]
        fn model_index(
            self: &ChannelModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
        #[cfg(feature = "qml_tests")]
        #[qinvokable]
        fn load_test(self: Pin<&mut ChannelModel>, json: QString) -> bool;

        #[qobject]
        #[qml_element]
        #[base = ChannelFilterProxy]
        #[qproperty(QString, band)]
        #[qproperty(QList_i32, visible_indices)]
        #[qproperty(Visibility, visibility)]
        #[qproperty(i32, opening_index)]
        #[qproperty(i32, count, READ = count, NOTIFY = changed)]
        #[qproperty(u32, revision, READ, NOTIFY = changed)]
        type ChannelFilterModel = super::ChannelFilter;
        fn count(self: &ChannelFilterModel) -> i32;
        #[qinvokable]
        fn row(self: &ChannelFilterModel, index: i32) -> QVariant;
        #[qinvokable]
        fn row_for_channel(self: &ChannelFilterModel, index: i32) -> i32;
        #[cxx_override]
        #[cxx_name = "filterAcceptsRow"]
        fn filter_accepts_row(self: &ChannelFilterModel, row: i32, parent: &QModelIndex) -> bool;
        #[inherit]
        #[cxx_name = "refreshFilter"]
        fn invalidate_filter(self: Pin<&mut ChannelFilterModel>);
        #[qsignal]
        fn changed(self: Pin<&mut ChannelFilterModel>);
        #[cxx_name = "notifyChanged"]
        fn notify_changed(self: Pin<&mut ChannelFilterModel>);
    }
    #[qenum(ChannelFilterModel)]
    enum Visibility {
        All,
        Listed,
    }
    impl cxx_qt::Initialize for ChannelFilterModel {}
}

#[derive(Default)]
pub struct ChannelPresentation {
    rows: Vec<Row>,
    revision: u32,
}

#[cfg_attr(feature = "qml_tests", derive(serde::Deserialize))]
struct Row {
    index: i32,
    label: String,
    band: String,
    #[cfg_attr(feature = "qml_tests", serde(default))]
    logo: String,
    #[cfg_attr(feature = "qml_tests", serde(default))]
    service_id: String,
}

fn band_name(band: Band) -> &'static str {
    match band {
        Band::Terrestrial => "GR",
        Band::Bs => "BS",
        Band::Cs => "CS",
        Band::Sky => "SKY",
        Band::Other => "OTHER",
    }
}

impl ffi::ChannelModel {
    pub fn count(&self) -> i32 {
        i32::try_from(self.rust().rows.len())
            .expect("HTTP-bounded channel catalog fits Qt row count")
    }
    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() { 0 } else { self.count() }
    }
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::default();
        for (role, name) in [
            (INDEX, "channelIndex"),
            (LABEL, "label"),
            (BAND, "band"),
            (LOGO, "logo"),
            (SERVICE, "serviceId"),
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
        let Some(channel) = usize::try_from(index.row())
            .ok()
            .and_then(|row| self.rust().rows.get(row))
        else {
            return QVariant::default();
        };
        let value = match role {
            INDEX => return QVariant::from(&channel.index),
            LABEL => channel.label.clone(),
            BAND => channel.band.clone(),
            LOGO => channel.logo.clone(),
            SERVICE => channel.service_id.clone(),
            _ => return QVariant::default(),
        };
        QVariant::from(&QString::from(value))
    }
    pub fn row(&self, index: i32) -> QVariant {
        ffi::channel_row(self, index)
    }
    pub fn has_band(&self, band: QString) -> bool {
        let band = band.to_string();
        self.rust().rows.iter().any(|channel| channel.band == band)
    }
    pub fn row_for_channel(&self, index: i32) -> i32 {
        ffi::source_row_for_index(self, index, INDEX)
    }
    pub(crate) fn replace(self: Pin<&mut Self>, channels: Arc<[Channel]>, server: &str) {
        let server = server.trim_end_matches('/');
        let rows = channels
            .iter()
            .enumerate()
            .map(|(index, channel)| Row {
                index: i32::try_from(index).expect("bounded channel catalog"),
                label: channel.label.clone(),
                band: band_name(channel.band).into(),
                logo: if channel.has_logo_data {
                    format!("{server}/api/services/{}/logo", channel.id)
                } else {
                    String::new()
                },
                service_id: channel.id.to_string(),
            })
            .collect();
        self.replace_rows(rows);
    }
    fn replace_rows(mut self: Pin<&mut Self>, rows: Vec<Row>) {
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().rows = rows;
        let next_revision = self.rust().revision.wrapping_add(1);
        self.as_mut().rust_mut().revision = next_revision;
        self.as_mut().end_reset_model();
        self.changed();
    }
    #[cfg(feature = "qml_tests")]
    pub fn load_test(self: Pin<&mut Self>, json: QString) -> bool {
        match serde_json::from_str(json.to_string().as_str()) {
            Ok(rows) => {
                self.replace_rows(rows);
                true
            }
            Err(_) => false,
        }
    }
}

pub struct ChannelFilter {
    revision: u32,
    band: QString,
    visible_indices: QList<i32>,
    visibility: ffi::Visibility,
    opening_index: i32,
}
impl Default for ChannelFilter {
    fn default() -> Self {
        Self {
            revision: 0,
            band: QString::from("ALL"),
            visible_indices: QList::default(),
            visibility: ffi::Visibility::All,
            opening_index: -1,
        }
    }
}
impl ffi::ChannelFilterModel {
    pub fn notify_changed(mut self: Pin<&mut Self>) {
        let revision = self.rust().revision.wrapping_add(1);
        self.as_mut().rust_mut().revision = revision;
        self.changed();
    }
    pub fn count(&self) -> i32 {
        ffi::filtered_count(self)
    }
    pub fn row(&self, index: i32) -> QVariant {
        ffi::filtered_row(self, index)
    }
    pub fn row_for_channel(&self, index: i32) -> i32 {
        ffi::row_for_index(self, index, INDEX)
    }
    pub fn filter_accepts_row(&self, row: i32, parent: &QModelIndex) -> bool {
        if parent.is_valid() {
            return false;
        }
        let band = self.band().to_string();
        let channel_index = ffi::source_data(self, row, INDEX).value::<i32>();
        (band == "ALL"
            || ffi::source_data(self, row, BAND)
                .value::<QString>()
                .is_some_and(|value| value.to_string() == band))
            && (match self.rust().visibility {
                ffi::Visibility::All => true,
                ffi::Visibility::Listed => {
                    channel_index == Some(self.rust().opening_index)
                        || self
                            .rust()
                            .visible_indices
                            .iter()
                            .any(|index| Some(*index) == channel_index)
                }
                _ => false, // Unknown Qt enum values never widen the visible set.
            })
    }
}
impl cxx_qt::Initialize for ffi::ChannelFilterModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut()
            .on_band_changed(|model| model.invalidate_filter())
            .release();
        self.as_mut()
            .on_visible_indices_changed(|model| model.invalidate_filter())
            .release();
        self.as_mut()
            .on_visibility_changed(|model| model.invalidate_filter())
            .release();
        self.as_mut()
            .on_opening_index_changed(|model| model.invalidate_filter())
            .release();
        ffi::connect_changes(self);
    }
}
