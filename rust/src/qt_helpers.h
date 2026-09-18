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
#include <QtGui/QGuiApplication>
#include <QtGui/QImage>
#include <QtGui/QImageWriter>
inline QString picturesDirectory() {
  return QStandardPaths::writableLocation(QStandardPaths::PicturesLocation);
}
// QImage's copy constructor shares immutable pixels. cxx-qt-lib's Clone makes
// a full pixel copy, which would unnecessarily hold up the UI for large captures.
inline QImage shareScreenshotImage(const QImage &image) { return image; }
inline bool saveScreenshotImage(const QImage &image, const QString &path,
                                const QString &format, int quality, int compression) {
  QImageWriter writer(path, format.toLatin1());
  writer.setQuality(quality);
  if (compression >= 0) writer.setCompression(compression);
  return writer.write(image);
}
inline void configureQtQuickOpenGl() {
    QCoreApplication::setApplicationName(QStringLiteral("nagametv"));
    QGuiApplication::setDesktopFileName(QStringLiteral("io.github.ouvill.nagametv"));
    QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);
}
inline void useQtQuickDialogs() {
    QCoreApplication::setAttribute(Qt::AA_DontUseNativeDialogs);
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
  return QDir(base).filePath("nagametv");
#else
  const QString base = QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation);
  if (base.isEmpty()) return {};
  return QDir(base).filePath("State");
#endif
}

inline bool openLocalDirectory(const QString &path) {
  return QDesktopServices::openUrl(QUrl::fromLocalFile(path));
}

// Installed once at startup, before Qt or playback creates worker threads.
#include "rust/cxx.h"
#include <QtCore/QLoggingCategory>
#include <optional>
#include <cstring>

inline bool qtGcEnabled = false;
inline std::optional<rust::Fn<void(rust::Str, rust::Str)>> qtGcCallback;
inline std::optional<rust::Fn<void(std::uint8_t, rust::Str, rust::Str)>> qtLogCallback;

inline void viewerQtMessageHandler(QtMsgType type, const QMessageLogContext &context,
                                   const QString &message) {
  const bool isGc = context.category &&
      (std::strcmp(context.category, "qt.qml.gc.statistics") == 0 ||
       std::strcmp(context.category, "qt.qml.gc.allocatorStats") == 0);
  // An explicit application switch wins even over QT_LOGGING_RULES.
  if (isGc && !qtGcEnabled && type != QtFatalMsg) return;
  if (isGc && qtGcEnabled && qtGcCallback) {
    const auto text = message.left(8192).toUtf8();
    (*qtGcCallback)(rust::Str(context.category), rust::Str(text.constData(), text.size()));
  }
  if (qtLogCallback) {
    const auto text = message.toUtf8();
    const char *category = context.category ? context.category : "default";
    std::uint8_t level;
    switch (type) {
      case QtDebugMsg: level = 0; break;
      case QtInfoMsg: level = 1; break;
      case QtWarningMsg: level = 2; break;
      case QtCriticalMsg: level = 3; break;
      case QtFatalMsg: level = 4; break;
      default: level = 3; break;
    }
    (*qtLogCallback)(level, rust::Str(category), rust::Str(text.constData(), text.size()));
  }
}

inline void installQtLogging(rust::Fn<void(std::uint8_t, rust::Str, rust::Str)> callback) {
  qtLogCallback = callback;
  qtGcEnabled = qEnvironmentVariable("NAGAMETV_GC_LOG") == QStringLiteral("1");
  qInstallMessageHandler(viewerQtMessageHandler);
}

inline void installQtGcLogging(rust::Fn<void(rust::Str, rust::Str)> callback) {
  qtGcCallback = callback;
  if (qEnvironmentVariable("NAGAMETV_GC_LOG") == QStringLiteral("1")) {
    // Explicit QT_LOGGING_RULES/QT_LOGGING_CONF still take precedence.
    QLoggingCategory::setFilterRules(QStringLiteral(
        "qt.qml.gc.statistics.debug=true\nqt.qml.gc.allocatorStats.debug=true"));
  }
}

#endif
