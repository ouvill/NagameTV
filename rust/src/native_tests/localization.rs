use super::bridge::ffi;
use crate::player::ffi::{
    apply_ui_language, current_ui_language, initialize_ui_language, translate_backend,
};
use cxx_qt_lib::{
    QByteArray, QCoreApplication, QMap, QMapPair_QString_QVariant, QQmlApplicationEngine, QString,
    QUrl, QVariant,
};

fn qs(text: &str) -> QString {
    QString::from(text)
}
fn tr(text: &str) -> QString {
    translate_backend(&qs(text))
}
fn property(engine: &QQmlApplicationEngine, name: &str) -> QVariant {
    ffi::root_property(engine, &qs(name))
}
fn text(engine: &QQmlApplicationEngine, name: &str) -> QString {
    property(engine, name)
        .value::<QString>()
        .expect("String QML property")
}
fn metric(stopping: bool) -> QString {
    let mut result = tr(
        "Subtitles: subscriptions %1, pending %2, received %3 | EPG: tasks %4, programs %5, stopping %6",
    );
    for argument in ["8", "2", "18446744073709551615", "1", "14019"] {
        result = result.arg(&qs(argument));
    }
    result.arg(&tr(if stopping { "Yes" } else { "No" }))
}

struct DisabledCatalog;
impl Drop for DisabledCatalog {
    fn drop(&mut self) {
        ffi::enable_catalog();
    }
}

pub fn run(missing_catalog: bool) -> i32 {
    // QtObject + QCoreApplication only; no GUI, GPU or audio required.
    let app = QCoreApplication::new();
    assert!(!app.is_null());
    let mut engine = QQmlApplicationEngine::new();
    if missing_catalog {
        assert!(
            ffi::catalog_exists(),
            "Catalog removal must start from a real resource"
        );
        ffi::disable_catalog();
        let _restore = DisabledCatalog;
        assert!(!ffi::catalog_exists(), "Catalog must actually be absent");
        assert!(initialize_ui_language(engine.pin_mut(), &qs("en")));
        assert!(
            apply_ui_language(&qs("ja")).is_empty(),
            "Missing catalog must fail"
        );
        assert_eq!(
            current_ui_language(),
            qs("en"),
            "Failure preserves active language"
        );
        assert_eq!(
            ffi::translator_count(),
            0,
            "Failed translator must be released"
        );
        println!("Missing catalog checks passed");
        return 0;
    }
    for locale in ["en_US", "fr_FR", "de_DE", "zh_CN", "C", ""] {
        assert_eq!(ffi::resolve_language(&qs("system"), &qs(locale)), qs("en"));
    }
    for (preference, locale, expected) in [
        ("system", "ja_JP", "ja"),
        ("ja", "en_US", "ja"),
        ("en", "ja_JP", "en"),
        ("unsupported", "ja_JP", "en"),
    ] {
        assert_eq!(
            ffi::resolve_language(&qs(preference), &qs(locale)),
            qs(expected)
        );
    }
    ffi::set_default_locale(&qs("de_DE"));
    assert!(initialize_ui_language(engine.pin_mut(), &qs("en")));
    engine.pin_mut().load_data(
        &QByteArray::from(include_str!("../../../tests/localization.qml")),
        &QUrl::from("file:///Main.qml"),
    );
    assert_eq!(ffi::root_count(&engine), 1);
    assert_eq!(text(&engine, "heading"), qs("Stats for nerds"));
    assert_eq!(text(&engine, "stateLabel"), qs("Playing"));
    let day = property(&engine, "day");
    assert_eq!(text(&engine, "dateLabel"), qs("Tue, Sep 8"));
    assert_eq!(tr("Programs: %1").arg(&qs("13000")), qs("Programs: 13000"));
    assert_eq!(
        metric(false),
        qs(
            "Subtitles: subscriptions 8, pending 2, received 18446744073709551615 | EPG: tasks 1, programs 14019, stopping No"
        )
    );
    assert_eq!(apply_ui_language(&qs("ja")), qs("ja"));
    assert_eq!(
        metric(true),
        qs(
            "字幕: 購読 8, 待機 2, 受信 18446744073709551615 | EPG: タスク 1, 番組 14019, 停止待ち はい"
        )
    );
    assert!(metric(false).to_string().ends_with("停止待ち いいえ"));
    assert_eq!(tr("Programs: %1").arg(&qs("13000")), qs("13000 番組"));
    assert_eq!(tr("Waiting to reconnect"), qs("再接続待ち"));
    assert_eq!(
        text(&engine, "failureLabel"),
        translate_backend(&text(&engine, "failureSource"))
    );
    assert_ne!(
        text(&engine, "failureLabel"),
        text(&engine, "failureSource")
    );
    assert_eq!(text(&engine, "stateLabel"), qs("再生中"));
    let mut snapshot = QMap::<QMapPair_QString_QVariant>::default();
    snapshot.insert(qs("state"), QVariant::from(&qs("Paused")));
    assert!(ffi::set_root_property(
        engine.pin_mut(),
        &qs("snapshot"),
        &QVariant::from(&snapshot)
    ));
    assert_eq!(text(&engine, "stateLabel"), qs("一時停止中"));
    assert_eq!(
        tr("Fetch failed: %1").arg(&qs("source %1 <tag> 日本語")),
        qs("取得失敗: source %1 <tag> 日本語")
    );
    assert_eq!(text(&engine, "dateLabel"), qs("9/8（火）"));
    assert_eq!(property(&engine, "day"), day);
    assert_eq!(text(&engine, "heading"), qs("動画統計"));
    assert_eq!(text(&engine, "closeLabel"), qs("閉じる"));
    assert_eq!(text(&engine, "emptyChannels"), qs("該当するチャンネルなし"));
    assert_eq!(tr("Loading channels..."), qs("チャンネルを取得中…"));
    assert_eq!(apply_ui_language(&qs("en")), qs("en"));
    assert!(metric(true).to_string().ends_with("stopping Yes"));
    assert_eq!(text(&engine, "heading"), qs("Stats for nerds"));
    assert_eq!(text(&engine, "dateLabel"), qs("Tue, Sep 8"));
    assert_eq!(text(&engine, "stateLabel"), qs("Paused"));
    assert_eq!(tr("Waiting to reconnect"), qs("Waiting to reconnect"));
    assert_eq!(
        text(&engine, "failureLabel"),
        text(&engine, "failureSource")
    );
    assert_eq!(text(&engine, "emptyChannels"), qs("No matching channels"));
    assert_eq!(apply_ui_language(&qs("ja")), qs("ja"));
    assert_eq!(text(&engine, "heading"), qs("動画統計"));
    for _ in 0..100 {
        assert_eq!(apply_ui_language(&qs("en")), qs("en"));
        assert_eq!(apply_ui_language(&qs("ja")), qs("ja"));
    }
    assert_eq!(
        ffi::translator_count(),
        1,
        "Only one translator may be cached"
    );
    println!("Localization smoke checks passed");
    0
}
