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
  for (const auto &language : {"en_US", "fr_FR", "de_DE", "zh_CN", "C", ""})
    check(resolveUiLanguage("system", language) == "en", "Unsupported locale must use English");
  check(resolveUiLanguage("system", "ja_JP") == "ja", "Japanese system locale");
  check(resolveUiLanguage("ja", "en_US") == "ja", "Explicit Japanese override");
  check(resolveUiLanguage("en", "ja_JP") == "en", "Explicit English override");
  check(resolveUiLanguage("unsupported", "ja_JP") == "en", "Unknown preference uses English");
  check(initializeUiLanguage(engine, "en"), "Initialize English");
  engine.loadData(R"(import QtQml
    QtObject { property string heading: qsTr("Stats for nerds") })", QUrl("file:///Main.qml"));
  check(engine.rootObjects().size() == 1, "Load translation test object");
  auto *root = engine.rootObjects().first();
  check(root->property("heading").toString() == "Stats for nerds", "English source text");
  check(applyUiLanguage("ja") == "ja", "Load Japanese catalog");
  check(root->property("heading").toString() == QString::fromUtf8("動画統計"), "Retranslate existing QML to Japanese");
  check(QCoreApplication::translate("Backend", "Loading channels...") == QString::fromUtf8("チャンネルを取得中…"), "Japanese backend message");
  check(applyUiLanguage("en") == "en", "Switch back to English");
  check(root->property("heading").toString() == "Stats for nerds", "Retranslate existing QML to English");
  check(applyUiLanguage("ja") == "ja", "Reuse Japanese translator");
  check(root->property("heading").toString() == QString::fromUtf8("動画統計"), "Switch repeatedly");
  qInfo() << "Localization smoke test passed";
}
