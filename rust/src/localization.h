#ifndef MIRAKURUN_VIEWER_LOCALIZATION_H
#define MIRAKURUN_VIEWER_LOCALIZATION_H

#include <QtCore/QCoreApplication>
#include <QtCore/QLocale>
#include <QtCore/QPointer>
#include <QtCore/QStringList>
#include <QtCore/QTranslator>
#include <QtQml/QQmlApplicationEngine>

// Extraction markers for the status strings emitted by the Rust backend.
inline constexpr const char *backendTranslationSources[] = {
    QT_TRANSLATE_NOOP("Backend", "Select a channel"),
    QT_TRANSLATE_NOOP("Backend", "Enter a server URL starting with http:// or https://"),
    QT_TRANSLATE_NOOP("Backend", "No available channels were found"),
    QT_TRANSLATE_NOOP("Backend", "Comments are unavailable for this channel"),
    QT_TRANSLATE_NOOP("Backend", "Connecting to comments…"),
    QT_TRANSLATE_NOOP("Backend", "Receiving comments"),
    QT_TRANSLATE_NOOP("Backend", "Connection ended"),
    QT_TRANSLATE_NOOP("Backend", "No active comment thread"),
    QT_TRANSLATE_NOOP("Backend", "Ready"),
    QT_TRANSLATE_NOOP("Backend", "Connecting..."),
    QT_TRANSLATE_NOOP("Backend", "Stopped"),
    QT_TRANSLATE_NOOP("Backend", "Playing"),
    QT_TRANSLATE_NOOP("Backend", "Paused"),
    QT_TRANSLATE_NOOP("Backend", "Null"),
    QT_TRANSLATE_NOOP("Backend", "Stream ended"),
    QT_TRANSLATE_NOOP("Backend", "Loading channels..."),
    QT_TRANSLATE_NOOP("Backend", "Network runtime is unavailable"),
    QT_TRANSLATE_NOOP("Backend", "Could not load UI translation"),
    QT_TRANSLATE_NOOP("Backend", "No tuner is available. Tuners may be in use or unavailable. Wait a moment and try again, or choose another channel."),
    QT_TRANSLATE_NOOP("Backend", "This channel was not found on Mirakurun. Refresh the channel list and choose a channel again."),
    QT_TRANSLATE_NOOP("Backend", "Mirakurun denied access to the stream. Check the server's access settings."),
    QT_TRANSLATE_NOOP("Backend", "The server did not respond in time. Check the connection and try again."),
    QT_TRANSLATE_NOOP("Backend", "Mirakurun could not start the stream. Check the server and try again."),
    QT_TRANSLATE_NOOP("Backend", "Mirakurun rejected the stream request. Check the connection settings and channel."),
    QT_TRANSLATE_NOOP("Backend", "Could not receive the stream from Mirakurun. Check the server and network connection, then try again."),
    QT_TRANSLATE_NOOP("Backend", "Enter a valid Mirakurun service ID"),
    QT_TRANSLATE_NOOP("Backend", "The live stream ended unexpectedly. Try again to reconnect."),
    QT_TRANSLATE_NOOP("Backend", "Could not play this channel. Try again or choose another channel. See the error details if the problem continues."),
    QT_TRANSLATE_NOOP("Backend", "Could not initialize the player"),
    QT_TRANSLATE_NOOP("Backend", "This audio track is no longer available. Choose a track again."),
    QT_TRANSLATE_NOOP("Backend", "Could not switch audio tracks. Try again."),
};

// English is the source language. Unsupported languages always resolve to it.
inline QString resolveUiLanguage(const QString &preference, const QString &systemLanguage) {
  const QString language = (preference == "system" ? systemLanguage : preference).toLower();
  return language == "ja" || language.startsWith("ja-") || language.startsWith("ja_")
      ? QStringLiteral("ja") : QStringLiteral("en");
}

inline QPointer<QQmlApplicationEngine> translationEngine;
inline QPointer<QTranslator> japaneseTranslator;
inline QString effectiveUiLanguage = QStringLiteral("en");

inline QString applyUiLanguage(const QString &preference) {
  const QLocale systemLocale = QLocale::system();
  const QString language = resolveUiLanguage(preference,
      systemLocale.uiLanguages().value(0, systemLocale.name()));
  if (language == "ja" && !japaneseTranslator) {
    auto *translator = new QTranslator(QCoreApplication::instance());
    if (!translator->load(QStringLiteral(":/i18n/ja.qm"))) {
      delete translator;
      return {}; // Do not save a selection which could not be applied.
    }
    japaneseTranslator = translator;
  }
  if (japaneseTranslator) QCoreApplication::removeTranslator(japaneseTranslator);
  if (language == "ja") QCoreApplication::installTranslator(japaneseTranslator);
  effectiveUiLanguage = language;
  if (translationEngine) {
    translationEngine->setUiLanguage(language);
    translationEngine->retranslate();
  }
  return language;
}

inline bool initializeUiLanguage(QQmlApplicationEngine &engine, const QString &preference) {
  translationEngine = &engine;
  return !applyUiLanguage(preference).isEmpty();
}

inline QString currentUiLanguage() { return effectiveUiLanguage; }

#endif // MIRAKURUN_VIEWER_LOCALIZATION_H
