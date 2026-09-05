#pragma once

#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>
#include <QtCore/QDir>
#include <QtCore/QCoreApplication>
#include <QtCore/QStandardPaths>
#include <QtCore/QUrl>
#include <QtGui/QDesktopServices>

#include <cstdint>

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
