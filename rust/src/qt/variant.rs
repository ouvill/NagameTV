//! Convert one presentation record to Qt values without JSON strings in QML.
use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};
pub fn value(json: serde_json::Value) -> QVariant {
    match json {
        serde_json::Value::Null => QVariant::default(),
        serde_json::Value::Bool(v) => QVariant::from(&v),
        serde_json::Value::Number(v) => {
            // Presentation times fit JS Date; opaque identities are separate strings.
            QVariant::from(&v.as_f64().expect("finite JSON number"))
        }
        serde_json::Value::String(v) => QVariant::from(&QString::from(v)),
        serde_json::Value::Array(values) => {
            let mut result = QList::<QVariant>::default();
            for item in values {
                result.append(value(item));
            }
            QVariant::from(&result)
        }
        serde_json::Value::Object(values) => {
            let mut result = QMap::<QMapPair_QString_QVariant>::default();
            for (key, item) in values {
                result.insert(QString::from(key), value(item));
            }
            QVariant::from(&result)
        }
    }
}
