#pragma once

#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>

#include <cstdint>

inline void configureQtQuickOpenGl() {
  QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);
}

inline std::uintptr_t qQuickItemAddress(QQuickItem *item) {
  return reinterpret_cast<std::uintptr_t>(item);
}
