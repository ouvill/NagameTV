#include "localization.h"
#include <QtCore/QDebug>
#include <cstdlib>

static void check(bool passed, const char *description) {
  if (!passed) {
    qCritical() << description;
    std::exit(1);
  }
}

int main(int argc, char **argv) {
  // Only QtObject and QCoreApplication: no GUI, display, GPU or audio required.
  QCoreApplication app(argc, argv);
  QQmlApplicationEngine engine;
#ifdef LOCALIZATION_WITHOUT_CATALOG
  check(initializeUiLanguage(engine, "en"), "English needs no catalog");
  check(applyUiLanguage("ja").isEmpty(), "Missing catalog must report failure");
  check(currentUiLanguage() == "en", "Failure must retain the effective language");
  check(app.findChildren<QTranslator *>().isEmpty(), "Failed translator must be released");
  qInfo() << "Missing catalog test passed";
  return 0;
#endif
  for (const auto &language : {"en_US", "fr_FR", "de_DE", "zh_CN", "C", ""})
    check(resolveUiLanguage("system", language) == "en", "Unsupported locale must use English");
  check(resolveUiLanguage("system", "ja_JP") == "ja", "Japanese system locale");
  check(resolveUiLanguage("ja", "en_US") == "ja", "Explicit Japanese override");
  check(resolveUiLanguage("en", "ja_JP") == "en", "Explicit English override");
  check(resolveUiLanguage("unsupported", "ja_JP") == "en", "Unknown preference uses English");
  // Deliberately differ from both supported UI languages to detect OS locale leakage.
  QLocale::setDefault(QLocale("de_DE"));
  check(initializeUiLanguage(engine, "en"), "Initialize English");
  engine.loadData(R"(import QtQml
    QtObject {
      readonly property var day: new Date(2026, 8, 8, 12, 0, 0)
      readonly property string dateLabel: day.toLocaleDateString(Qt.locale(Qt.uiLanguage), qsTranslate("Main", "ddd, MMM d"))
      property string failureSource: "This channel was not found on Mirakurun. Refresh the channel list and choose a channel again."
      readonly property string failureLabel: qsTranslate("Backend", failureSource)
      property var snapshot: ({state: "Playing"})
      function metric(key) {
        const s = snapshot
        return key === "state" && s.state ? qsTranslate("Backend", s.state) : "—"
      }
      readonly property string stateLabel: metric("state")
      property string heading: qsTr("Stats for nerds"); property string closeLabel: qsTranslate("Main", "Close"); property string emptyChannels: qsTranslate("Viewer", "No matching channels") })", QUrl("file:///Main.qml"));
  check(engine.rootObjects().size() == 1, "Load translation test object");
  auto *root = engine.rootObjects().first();
  check(root->property("heading").toString() == "Stats for nerds", "English source text");
  check(root->property("stateLabel").toString() == "Playing", "State through a function starts in English");
  const auto dayBeforeSwitch = root->property("day");
  check(root->property("dateLabel").toString() == "Tue, Sep 8", "English date independent of system locale");
  check(translateBackend("Programs: %1").arg("13000") == "Programs: 13000", "English feature count");
  const auto metricText = [](bool stopping) {
    return translateBackend("Subtitles: subscriptions %1, pending %2, received %3 | EPG: tasks %4, programs %5, stopping %6")
        .arg("8").arg("2").arg("18446744073709551615").arg("1").arg("14019")
        .arg(translateBackend(stopping ? "Yes" : "No"));
  };
  check(metricText(false) == "Subtitles: subscriptions 8, pending 2, received 18446744073709551615 | EPG: tasks 1, programs 14019, stopping No", "English diagnostic template preserves all counters");
  check(applyUiLanguage("ja") == "ja", "Load Japanese catalog");
  check(metricText(true) == QString::fromUtf8("字幕: 購読 8, 待機 2, 受信 18446744073709551615 | EPG: タスク 1, 番組 14019, 停止待ち はい"), "Japanese diagnostic template and stopping state");
  check(metricText(false).endsWith(QString::fromUtf8("停止待ち いいえ")), "Japanese non-stopping state");
  check(translateBackend("Programs: %1").arg("13000") == QString::fromUtf8("13000 番組"), "Japanese feature count template reorders argument");
  check(translateBackend("Waiting to reconnect") == QString::fromUtf8("再接続待ち"), "Japanese retry status");
  check(root->property("failureLabel").toString() == translateBackend(root->property("failureSource").toString()), "Dynamic failure source is retranslated");
  check(root->property("failureLabel") != root->property("failureSource"), "Failure guidance has a Japanese translation");
  check(root->property("stateLabel").toString() == QString::fromUtf8("再生中"), "Function translation updates without a new snapshot");
  check(root->setProperty("snapshot", QVariantMap{{"state", "Paused"}}), "Replace statistics snapshot");
  check(root->property("stateLabel").toString() == QString::fromUtf8("一時停止中"), "New snapshot translates in the active language");
  const QString detail = QString::fromUtf8("source %1 <tag> 日本語");
  check(translateBackend("Fetch failed: %1").arg(detail) == QString::fromUtf8("取得失敗: ") + detail, "Diagnostics remain literal data including placeholders");
  check(root->property("dateLabel").toString() == QString::fromUtf8("9/8（火）"), "Japanese date and translated format update together");
  check(root->property("day") == dayBeforeSwitch, "Language change preserves the date");
  check(root->property("heading").toString() == QString::fromUtf8("動画統計"), "Retranslate existing QML to Japanese");
  check(root->property("closeLabel").toString() == QString::fromUtf8("閉じる"), "Explicit shared translation context");
  check(root->property("emptyChannels").toString() == QString::fromUtf8("該当するチャンネルなし"), "Feature UI catalog context");
  check(QCoreApplication::translate("Backend", "Loading channels...") == QString::fromUtf8("チャンネルを取得中…"), "Japanese backend message");
  check(applyUiLanguage("en") == "en", "Switch back to English");
  check(metricText(true).endsWith("stopping Yes"), "Existing counters return to English");
  check(root->property("heading").toString() == "Stats for nerds", "Retranslate existing QML to English");
  check(root->property("dateLabel").toString() == "Tue, Sep 8", "Date returns to English");
  check(root->property("stateLabel").toString() == "Paused", "Function translation returns to English without polling");
  check(translateBackend("Waiting to reconnect") == "Waiting to reconnect", "Feature status returns to English");
  check(root->property("failureLabel") == root->property("failureSource"), "Existing failure guidance returns to English");
  check(root->property("emptyChannels").toString() == "No matching channels", "Feature UI context returns to English");
  check(applyUiLanguage("ja") == "ja", "Reuse Japanese translator");
  check(root->property("heading").toString() == QString::fromUtf8("動画統計"), "Switch repeatedly");
  for (int i = 0; i < 100; ++i) {
    check(applyUiLanguage("en") == "en", "Repeated English switch");
    check(applyUiLanguage("ja") == "ja", "Repeated Japanese switch");
  }
  check(app.findChildren<QTranslator *>().size() == 1, "Only one cached translator is owned");
  qInfo() << "Localization smoke test passed";
}
