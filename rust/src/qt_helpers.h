#ifndef MINIMAL_VIEWER_QT_HELPERS_H
#define MINIMAL_VIEWER_QT_HELPERS_H
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>
#include <cstdint>
#include <QtCore/QCoreApplication>
#include <QtCore/QDir>
#include <QtCore/QStandardPaths>
#include <QtCore/QUrl>
#include <QtGui/QDesktopServices>
inline void configureQtQuickOpenGl() {
    QCoreApplication::setApplicationName(QStringLiteral("mirakurun-viewer"));
    QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);
}
inline std::uintptr_t qQuickItemAddress(QQuickItem *item) {
    return reinterpret_cast<std::uintptr_t>(item);
}
inline QString playbackLogDirectory() {
#if QT_VERSION >= QT_VERSION_CHECK(6, 7, 0)
  return QStandardPaths::writableLocation(QStandardPaths::StateLocation);
#elif defined(Q_OS_LINUX)
  // StateLocation is unavailable in the minimum supported Qt version.
  QString base = qEnvironmentVariable("XDG_STATE_HOME");
  if (base.isEmpty() || !QDir::isAbsolutePath(base)) {
    const QString home = QDir::homePath();
    if (home.isEmpty() || !QDir::isAbsolutePath(home)) return {};
    base = QDir(home).filePath(".local/state");
  }
  return QDir(base).filePath("mirakurun-viewer");
#else
  const QString base = QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation);
  if (base.isEmpty()) return {};
  return QDir(base).filePath("State");
#endif
}

inline bool openPlaybackLogDirectory(const QString &path) {
  return QDesktopServices::openUrl(QUrl::fromLocalFile(path));
}

// Installed once at startup, before Qt or playback creates worker threads.
#include "rust/cxx.h"
#include <QtCore/QLoggingCategory>
#include <optional>
#include <cstdio>
#include <cstring>

inline std::optional<rust::Fn<void(rust::Str, rust::Str)>> qtGcCallback;
inline QtMessageHandler previousQtHandler = nullptr;

inline void viewerQtMessageHandler(QtMsgType type, const QMessageLogContext &context,
                                   const QString &message) {
  if (context.category && qtGcCallback &&
      (std::strcmp(context.category, "qt.qml.gc.statistics") == 0 ||
       std::strcmp(context.category, "qt.qml.gc.allocatorStats") == 0)) {
    const auto text = message.left(8192).toUtf8();
    (*qtGcCallback)(rust::Str(context.category), rust::Str(text.constData(), text.size()));
  }
  if (previousQtHandler) previousQtHandler(type, context, message);
  else {
    const auto text = qFormatLogMessage(type, context, message).toLocal8Bit();
    if (!text.isEmpty()) std::fprintf(stderr, "%s\n", text.constData());
  }
}

inline void installQtGcLogging(rust::Fn<void(rust::Str, rust::Str)> callback) {
  qtGcCallback = callback;
  previousQtHandler = qInstallMessageHandler(viewerQtMessageHandler);
  if (qEnvironmentVariable("MIRAKURUN_GC_LOG") == QStringLiteral("1")) {
    // Explicit QT_LOGGING_RULES/QT_LOGGING_CONF still take precedence.
    QLoggingCategory::setFilterRules(QStringLiteral(
        "qt.qml.gc.statistics.debug=true\nqt.qml.gc.allocatorStats.debug=true"));
  }
}

#endif
