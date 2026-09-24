//! Qt projections of validated schedules and selected conflict candidates.
use crate::features::program_info::{
    model::Snapshot,
    schedule::{End, Resolution},
    view::View,
    watch::Action,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use std::pin::Pin;
#[cfg(feature = "native_tests")]
pub mod checks;
#[cfg(feature = "qml_tests")]
mod fixtures;

const CHANNEL: i32 = 256;
const BEGIN: i32 = 257;
const END: i32 = 258;
const KEY: i32 = 259;
const NAME: i32 = 260;
const DESCRIPTION: i32 = 261;
const GENRE: i32 = 262;
const STATE: i32 = 263;
const COUNT: i32 = 264;
const START: i32 = 265;
const DURATION: i32 = 266;

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
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("guide_model_helpers.h");
        #[rust_name = "make_guide_model"]
        fn makeGuideModel() -> UniquePtr<GuideModel>;
        #[rust_name = "make_candidates"]
        fn makeGuideCandidates() -> UniquePtr<GuideCandidates>;
        #[cxx_name = "channelRow"]
        fn model_row(model: &GuideModel, row: i32) -> QVariant;
        #[cxx_name = "channelRow"]
        fn candidate_row(model: &GuideCandidates, row: i32) -> QVariant;
        #[cxx_name = "channelSourceData"]
        fn guide_source_data(model: &GuideFilterModel, row: i32, role: i32) -> QVariant;
    }
    #[cfg(feature = "native_tests")]
    unsafe extern "C++" {
        include!("channel_model_test.h");
        #[cxx_name = "checkChannelModel"]
        fn check_guide_model(model: Pin<&mut GuideModel>);
        #[cxx_name = "checkChannelModel"]
        fn check_candidates(model: Pin<&mut GuideCandidates>);
        #[cxx_name = "checkChannelModel"]
        fn check_filter(model: Pin<&mut GuideFilterModel>);
        #[cxx_name = "setChannelTestSource"]
        fn set_source(filter: Pin<&mut GuideFilterModel>, source: Pin<&mut GuideModel>);
        #[rust_name = "make_filter"]
        fn makeGuideFilterModel() -> UniquePtr<GuideFilterModel>;
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(i32, count, READ = count, NOTIFY = changed)]
        #[qproperty(u32, revision, READ, NOTIFY = changed)]
        #[qproperty(*mut GuideCandidates, candidates, READ = candidates, CONSTANT)]
        type GuideModel = super::GuidePresentation;
        fn count(self: &GuideModel) -> i32;
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &GuideModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn data(self: &GuideModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &GuideModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        fn row(self: &GuideModel, index: i32) -> QVariant;
        #[qsignal]
        fn changed(self: Pin<&mut GuideModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut GuideModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut GuideModel>);
        #[inherit]
        #[cxx_name = "index"]
        fn model_index(
            self: &GuideModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(i32, count, READ = count, NOTIFY = changed)]
        #[qproperty(u32, revision, READ, NOTIFY = changed)]
        type GuideCandidates = super::Candidates;
        fn count(self: &GuideCandidates) -> i32;
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &GuideCandidates, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn data(self: &GuideCandidates, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &GuideCandidates) -> QHash_i32_QByteArray;
        #[qinvokable]
        fn row(self: &GuideCandidates, index: i32) -> QVariant;
        #[qsignal]
        fn changed(self: Pin<&mut GuideCandidates>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut GuideCandidates>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut GuideCandidates>);
        #[inherit]
        #[cxx_name = "index"]
        fn model_index(
            self: &GuideCandidates,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
        #[qinvokable]
        fn lookup(self: &GuideModel, key: QString) -> QVariant;
        #[qinvokable]
        fn nearest(self: &GuideModel, channel: i32, time: f64) -> QVariant;
        #[qinvokable]
        fn adjacent(self: &GuideModel, channel: i32, key: QString, step: i32) -> QVariant;
        #[qinvokable]
        fn action(self: &GuideModel, key: QString, now: f64) -> i32;
        #[qinvokable]
        fn details(self: &GuideModel, key: QString, candidate: i32) -> QVariant;
        #[qinvokable]
        fn select_candidates(self: Pin<&mut GuideModel>, key: QString);
        fn candidates(self: &GuideModel) -> *mut GuideCandidates;
        #[cfg(feature = "qml_tests")]
        #[qinvokable]
        fn load_test(self: Pin<&mut GuideModel>, json: QString) -> bool;
        #[cfg(feature = "qml_tests")]
        #[qinvokable]
        fn test_key(self: &GuideModel, alias: QString) -> QString;

        #[qobject]
        #[qml_element]
        #[base = ChannelFilterProxy]
        #[qproperty(i32, channel_index)]
        type GuideFilterModel = super::Filter;
        #[cxx_override]
        #[cxx_name = "filterAcceptsRow"]
        fn filter_accepts_row(self: &GuideFilterModel, row: i32, parent: &QModelIndex) -> bool;
        #[inherit]
        #[cxx_name = "refreshFilter"]
        fn invalidate_filter(self: Pin<&mut GuideFilterModel>);
    }
    #[qenum(GuideModel)]
    enum ScheduleState {
        Single,
        Conflict,
        UnknownEnd,
    }
    #[qenum(GuideModel)]
    enum WatchAction {
        Unavailable,
        WatchProgram,
        WatchChannel,
    }
    impl cxx_qt::Initialize for GuideFilterModel {}
}

pub struct GuidePresentation {
    #[cfg(feature = "qml_tests")]
    test_keys: std::collections::HashMap<String, String>,
    view: View,
    revision: u32,
    candidates: cxx::UniquePtr<ffi::GuideCandidates>,
}
impl Default for GuidePresentation {
    fn default() -> Self {
        Self {
            #[cfg(feature = "qml_tests")]
            test_keys: Default::default(),
            view: View::default(),
            revision: 0,
            candidates: ffi::make_candidates(),
        }
    }
}
#[derive(Default)]
pub struct Candidates {
    snapshot: Snapshot,
    indices: Vec<usize>,
    revision: u32,
}
#[derive(Default)]
pub struct Filter {
    channel_index: i32,
}
fn roles(values: &[(i32, &str)]) -> QHash<QHashPair_i32_QByteArray> {
    let mut result = QHash::default();
    for &(role, name) in values {
        result.insert(role, QByteArray::from(name));
    }
    result
}
fn text(value: &str) -> QVariant {
    QVariant::from(&QString::from(value))
}
fn state(view: &View, row: usize) -> ffi::ScheduleState {
    match view.cells()[row].segment.resolution {
        Resolution::Single(i) => match view.snapshot().program(i).end() {
            End::Known(_) => ffi::ScheduleState::Single,
            End::Unknown => ffi::ScheduleState::UnknownEnd,
        },
        Resolution::Conflict(_) => ffi::ScheduleState::Conflict,
        Resolution::Gap => unreachable!("gaps have no guide cells"),
    }
}
impl ffi::GuideModel {
    pub fn count(&self) -> i32 {
        self.rust().view.cells().len() as i32
    }
    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() { 0 } else { self.count() }
    }
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        roles(&[
            (CHANNEL, "channelIndex"),
            (BEGIN, "begin"),
            (END, "end"),
            (KEY, "watchKey"),
            (NAME, "name"),
            (DESCRIPTION, "description"),
            (GENRE, "genre"),
            (STATE, "scheduleState"),
            (COUNT, "candidateCount"),
            (START, "startAt"),
            (DURATION, "duration"),
        ])
    }
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        if !index.is_valid()
            || index.column() != 0
            || *index != self.model_index(index.row(), 0, &QModelIndex::default())
        {
            return QVariant::default();
        }
        let view = &self.rust().view;
        let Some(cell) = usize::try_from(index.row())
            .ok()
            .and_then(|i| view.cells().get(i))
        else {
            return QVariant::default();
        };
        let program = view.program(cell);
        match role {
            CHANNEL => QVariant::from(&(cell.channel as i32)),
            BEGIN => QVariant::from(&(cell.begin as f64)),
            END => QVariant::from(&(cell.end as f64)),
            KEY => text(&cell.key),
            NAME => text(program.and_then(|p| p.name.as_deref()).unwrap_or("")),
            DESCRIPTION => text(program.and_then(|p| p.description.as_deref()).unwrap_or("")),
            GENRE => QVariant::from(&program.map_or(15, |p| p.genre as i32)),
            STATE => QVariant::from(&(state(view, index.row() as usize).repr)),
            COUNT => QVariant::from(
                &(match cell.segment.resolution {
                    Resolution::Single(_) => 1,
                    Resolution::Conflict(count) => count,
                    Resolution::Gap => 0,
                } as i32),
            ),
            START => QVariant::from(&(program.map_or(cell.begin, |p| p.start_at) as f64)),
            DURATION => {
                QVariant::from(&(program.map_or(cell.end - cell.begin, |p| p.duration) as f64))
            }
            _ => QVariant::default(),
        }
    }
    pub fn row(&self, index: i32) -> QVariant {
        ffi::model_row(self, index)
    }
    pub fn lookup(&self, key: QString) -> QVariant {
        self.rust()
            .view
            .find(&key.to_string())
            .map_or_else(QVariant::default, |i| self.row(i as i32))
    }
    pub fn nearest(&self, channel: i32, time: f64) -> QVariant {
        if channel < 0 || !time.is_finite() || time < 0. {
            return QVariant::default();
        }
        self.rust()
            .view
            .nearest(channel as usize, time as u64)
            .map_or_else(QVariant::default, |i| self.row(i as i32))
    }
    pub fn adjacent(&self, channel: i32, key: QString, step: i32) -> QVariant {
        if channel < 0 {
            return QVariant::default();
        }
        self.rust()
            .view
            .adjacent(channel as usize, &key.to_string(), step)
            .map_or_else(QVariant::default, |i| self.row(i as i32))
    }
    pub fn action(&self, key: QString, now: f64) -> i32 {
        if !now.is_finite() || now < 0. {
            return ffi::WatchAction::Unavailable.repr;
        }
        match self.rust().view.action(&key.to_string(), now as u64) {
            None => ffi::WatchAction::Unavailable.repr,
            Some(Action::Program) => ffi::WatchAction::WatchProgram.repr,
            Some(Action::Channel) => ffi::WatchAction::WatchChannel.repr,
        }
    }
    pub fn candidates(&self) -> *mut ffi::GuideCandidates {
        self.rust().candidates.as_ptr().cast_mut()
    }
    pub fn select_candidates(mut self: Pin<&mut Self>, key: QString) {
        let indices = self.rust().view.candidates(&key.to_string());
        let snapshot = self.rust().view.snapshot().clone();
        self.as_mut()
            .rust_mut()
            .candidates
            .pin_mut()
            .replace(snapshot, indices);
    }
    pub fn details(&self, key: QString, candidate: i32) -> QVariant {
        if candidate < 0 {
            return QVariant::default();
        }
        let view = &self.rust().view;
        let Some(row) = view.find(&key.to_string()) else {
            return QVariant::default();
        };
        let indices = view.candidates(&key.to_string());
        let Some(&index) = indices.get(candidate as usize) else {
            return QVariant::default();
        };
        match serde_json::to_value(view.snapshot().program(index)) {
            Ok(mut value) => {
                value["id"] = view.snapshot().program(index).id.to_string().into();
                value["endUnknown"] = (view.snapshot().program(index).end() == End::Unknown).into();
                value["scheduleState"] = state(view, row).repr.into();
                crate::qt::variant::value(value)
            }
            Err(error) => {
                tracing::error!(
                    error = &error as &dyn std::error::Error,
                    "Guide detail projection failed"
                );
                QVariant::default()
            }
        }
    }
    pub(crate) fn replace(mut self: Pin<&mut Self>, view: View) {
        self.as_mut().begin_reset_model();
        {
            let mut this = self.as_mut().rust_mut();
            this.view = view;
            this.revision = this.revision.wrapping_add(1);
        }
        self.as_mut().end_reset_model();
        self.as_mut().select_candidates(QString::default());
        self.changed();
    }
}
impl ffi::GuideCandidates {
    pub fn count(&self) -> i32 {
        self.rust().indices.len() as i32
    }
    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() { 0 } else { self.count() }
    }
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        roles(&[(NAME, "name"), (START, "startAt"), (DURATION, "duration")])
    }
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        if !index.is_valid()
            || index.column() != 0
            || *index != self.model_index(index.row(), 0, &QModelIndex::default())
        {
            return QVariant::default();
        }
        let Some(&i) = usize::try_from(index.row())
            .ok()
            .and_then(|r| self.rust().indices.get(r))
        else {
            return QVariant::default();
        };
        let p = self.rust().snapshot.program(i);
        match role {
            NAME => text(p.name.as_deref().unwrap_or("")),
            START => QVariant::from(&(p.start_at as f64)),
            DURATION => QVariant::from(&(p.duration as f64)),
            _ => QVariant::default(),
        }
    }
    pub fn row(&self, index: i32) -> QVariant {
        ffi::candidate_row(self, index)
    }
    fn replace(mut self: Pin<&mut Self>, snapshot: Snapshot, indices: Vec<usize>) {
        self.as_mut().begin_reset_model();
        {
            let mut this = self.as_mut().rust_mut();
            this.snapshot = snapshot;
            this.indices = indices;
            this.revision = this.revision.wrapping_add(1);
        }
        self.as_mut().end_reset_model();
        self.changed();
    }
}
impl ffi::GuideFilterModel {
    pub fn filter_accepts_row(&self, row: i32, parent: &QModelIndex) -> bool {
        !parent.is_valid()
            && ffi::guide_source_data(self, row, CHANNEL).value::<i32>()
                == Some(self.rust().channel_index)
    }
}
impl cxx_qt::Initialize for ffi::GuideFilterModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut()
            .on_channel_index_changed(|m| m.invalidate_filter())
            .release();
    }
}
