//! The sole owner of comment history; Qt receives row changes, never JSON snapshots.
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use std::{collections::VecDeque, pin::Pin};
use viewer_comments::{Comment, Origin};

pub const HISTORY_LIMIT: usize = 2_000;
const ID: i32 = 256;
const TIME: i32 = 257;
const TEXT: i32 = 258;
const SOURCE: i32 = 259;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("QtCore/QAbstractListModel");
        type QAbstractListModel;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("comment_model_factory.h");
        #[rust_name = "make_comment_model"]
        fn makeCommentModel() -> UniquePtr<CommentModel>;
    }
    #[cfg(feature = "qml_tests")]
    unsafe extern "C++" {
        include!("comment_model_test.h");
        #[rust_name = "attach_model_tester"]
        fn attachCommentModelTester(model: Pin<&mut CommentModel>);
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(i32, count, READ = row_count_property, NOTIFY = count_changed)]
        type CommentModel = super::History;
        fn row_count_property(self: &CommentModel) -> i32;
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &CommentModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn data(self: &CommentModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &CommentModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        fn id_at(self: &CommentModel, row: i32) -> QString;
        #[qinvokable]
        fn row_for_id(self: &CommentModel, id: QString) -> i32;
        #[qsignal]
        fn count_changed(self: Pin<&mut CommentModel>);
        #[qsignal]
        fn about_to_update(self: Pin<&mut CommentModel>);
        #[qsignal]
        fn updated(self: Pin<&mut CommentModel>);
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        fn begin_insert_rows(
            self: Pin<&mut CommentModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endInsertRows"]
        fn end_insert_rows(self: Pin<&mut CommentModel>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        fn begin_remove_rows(
            self: Pin<&mut CommentModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        fn end_remove_rows(self: Pin<&mut CommentModel>);
        #[inherit]
        #[cxx_name = "index"]
        fn model_index(
            self: &CommentModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
        #[cfg(feature = "qml_tests")]
        #[qinvokable]
        fn enable_model_test(self: Pin<&mut CommentModel>);
        #[cfg(feature = "qml_tests")]
        #[qinvokable]
        fn append_batch_test(self: Pin<&mut CommentModel>, count: i32);
        #[cfg(feature = "qml_tests")]
        #[qinvokable]
        fn append_test(
            self: Pin<&mut CommentModel>,
            text: QString,
            unix_seconds: u64,
            source_nico: bool,
        );
        #[cfg(feature = "qml_tests")]
        #[qinvokable]
        fn clear_test(self: Pin<&mut CommentModel>);
    }
}

#[derive(Default)]
pub struct History {
    rows: VecDeque<Comment>,
    received: u64,
}

impl ffi::CommentModel {
    pub fn row_count_property(&self) -> i32 {
        self.rust().rows.len() as i32
    }
    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() {
            0
        } else {
            self.row_count_property()
        }
    }
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::default();
        for (role, name) in [
            (ID, "commentId"),
            (TIME, "time"),
            (TEXT, "text"),
            (SOURCE, "source"),
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
        let Some(comment) = usize::try_from(index.row())
            .ok()
            .and_then(|i| self.rust().rows.get(i))
        else {
            return QVariant::default();
        };
        let value = match role {
            ID => self.id_at(index.row()),
            TIME => QString::from(comment.japan_time()),
            TEXT => QString::from(comment.text.as_ref()),
            SOURCE => QString::from(match comment.origin {
                Origin::Nx => "NX",
                Origin::Niconico => "ニコ実",
            }),
            _ => return QVariant::default(),
        };
        QVariant::from(&value)
    }
    pub fn id_at(&self, row: i32) -> QString {
        if row < 0 || row >= self.row_count_property() {
            return QString::default();
        }
        QString::from(
            (self.rust().received - self.rust().rows.len() as u64 + row as u64).to_string(),
        )
    }
    pub fn row_for_id(&self, id: QString) -> i32 {
        id.to_string()
            .parse::<u64>()
            .ok()
            .and_then(|id| id.checked_sub(self.rust().received - self.rust().rows.len() as u64))
            .filter(|&row| row < self.rust().rows.len() as u64)
            .map_or(-1, |row| row as i32)
    }
    pub fn storage(&self) -> (usize, usize) {
        (
            self.rust().rows.len(),
            self.rust().rows.iter().map(|c| c.text.len()).sum(),
        )
    }
    pub fn append(mut self: Pin<&mut Self>, comments: Vec<Comment>) {
        if comments.is_empty() {
            return;
        }
        self.as_mut().about_to_update();
        let old_count = self.row_count_property();
        let incoming = comments.len();
        let remove = (self.rust().rows.len() + incoming)
            .saturating_sub(HISTORY_LIMIT)
            .min(self.rust().rows.len());
        let parent = QModelIndex::default();
        if remove > 0 {
            self.as_mut()
                .begin_remove_rows(&parent, 0, remove as i32 - 1);
            self.as_mut().rust_mut().rows.drain(..remove);
            self.as_mut().end_remove_rows();
        }
        let skip = incoming.saturating_sub(HISTORY_LIMIT);
        let first = self.row_count_property();
        self.as_mut()
            .begin_insert_rows(&parent, first, first + (incoming - skip) as i32 - 1);
        {
            let mut state = self.as_mut().rust_mut();
            state.received += incoming as u64;
            state.rows.extend(comments.into_iter().skip(skip));
        }
        self.as_mut().end_insert_rows();
        if old_count != self.row_count_property() {
            self.as_mut().count_changed();
        }
        self.updated();
    }
    pub fn clear(mut self: Pin<&mut Self>) {
        if self.rust().rows.is_empty() {
            return;
        }
        self.as_mut().about_to_update();
        let last = self.row_count_property() - 1;
        self.as_mut()
            .begin_remove_rows(&QModelIndex::default(), 0, last);
        self.as_mut().rust_mut().rows = VecDeque::new();
        self.as_mut().end_remove_rows();
        self.as_mut().count_changed();
        self.updated();
    }
    #[cfg(feature = "qml_tests")]
    pub fn append_test(self: Pin<&mut Self>, text: QString, unix_seconds: u64, source_nico: bool) {
        self.append(vec![Comment {
            text: text.to_string().into_boxed_str(),
            unix_seconds,
            origin: if source_nico {
                Origin::Niconico
            } else {
                Origin::Nx
            },
            phase: viewer_comments::Phase::Live,
            style: viewer_comments::Style::default(),
        }]);
    }
    #[cfg(feature = "qml_tests")]
    pub fn clear_test(self: Pin<&mut Self>) {
        self.clear();
    }
}

#[cfg(feature = "qml_tests")]
impl ffi::CommentModel {
    pub fn enable_model_test(self: Pin<&mut Self>) {
        ffi::attach_model_tester(self);
    }
    pub fn append_batch_test(self: Pin<&mut Self>, count: i32) {
        self.append(
            (0..count.clamp(0, 10_000))
                .map(|i| Comment {
                    text: format!("comment {i}").into_boxed_str(),
                    unix_seconds: i as u64,
                    origin: Origin::Nx,
                    phase: viewer_comments::Phase::History,
                    style: viewer_comments::Style::default(),
                })
                .collect(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn batch(start: usize, end: usize) -> Vec<Comment> {
        (start..end)
            .map(|i| Comment {
                text: format!("<b>{i}</b>\n日本語").into_boxed_str(),
                unix_seconds: i as u64,
                origin: if i % 2 == 0 {
                    Origin::Nx
                } else {
                    Origin::Niconico
                },
                phase: viewer_comments::Phase::Live,
                style: viewer_comments::Style::default(),
            })
            .collect()
    }
    #[test]
    fn bounded_batches_preserve_ids_and_release_on_clear() {
        let mut model = ffi::make_comment_model();
        model.pin_mut().append(batch(0, 1999));
        let retained = model.id_at(100);
        model.pin_mut().append(batch(1999, 2010));
        assert_eq!(model.storage().0, HISTORY_LIMIT);
        assert_eq!(model.id_at(0).to_string(), "10");
        assert_eq!(model.row_for_id(retained), 90);
        assert_eq!(model.row_for_id(QString::from("9")), -1);
        assert_eq!(model.row_for_id(QString::from("2010")), -1);
        assert_eq!(model.row_for_id(QString::from("invalid")), -1);
        model.pin_mut().append(batch(2010, 5010));
        assert_eq!(model.row_count_property(), 2000);
        assert_eq!(model.id_at(0).to_string(), "3010");
        assert_eq!(model.id_at(1999).to_string(), "5009");
        assert_eq!(model.rust().rows[0].text.as_ref(), "<b>3010</b>\n日本語");
        let old_id = model.id_at(1999);
        model.pin_mut().clear();
        assert_eq!(model.storage(), (0, 0));
        assert_eq!(model.rust().rows.capacity(), 0);
        model.pin_mut().clear();
        model.pin_mut().append(batch(0, 1));
        assert_eq!(model.row_for_id(old_id), -1);
        assert_eq!(model.id_at(0).to_string(), "5010");
    }
    #[test]
    fn qt_roles_are_typed_and_invalid_indices_are_empty() {
        let mut model = ffi::make_comment_model();
        model.pin_mut().append(batch(0, 2));
        let invalid = QModelIndex::default();
        let first = model.model_index(0, 0, &invalid);
        let second = model.model_index(1, 0, &invalid);
        assert_eq!(model.row_count(&invalid), 2);
        assert_eq!(model.row_count(&first), 0);
        assert_eq!(
            model
                .data(&first, TEXT)
                .value::<QString>()
                .unwrap()
                .to_string(),
            "<b>0</b>\n日本語"
        );
        assert_eq!(
            model
                .data(&first, TIME)
                .value::<QString>()
                .unwrap()
                .to_string(),
            "09:00:00"
        );
        assert_eq!(
            model
                .data(&first, SOURCE)
                .value::<QString>()
                .unwrap()
                .to_string(),
            "NX"
        );
        assert_eq!(
            model
                .data(&second, SOURCE)
                .value::<QString>()
                .unwrap()
                .to_string(),
            "ニコ実"
        );
        assert_eq!(
            model
                .data(&first, ID)
                .value::<QString>()
                .unwrap()
                .to_string(),
            "0"
        );
        assert_eq!(model.data(&invalid, TEXT), QVariant::default());
        assert_eq!(model.data(&first, 999), QVariant::default());
        assert_eq!(model.role_names().len(), 4);
    }
}
