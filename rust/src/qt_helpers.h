#ifndef MINIMAL_VIEWER_QT_HELPERS_H
#define MINIMAL_VIEWER_QT_HELPERS_H
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>
#include <cstdint>
inline void configureQtQuickOpenGl() {
    QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);
}
inline std::uintptr_t qQuickItemAddress(QQuickItem *item) {
    return reinterpret_cast<std::uintptr_t>(item);
}
#endif
