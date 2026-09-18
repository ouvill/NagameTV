#ifndef NAGAMETV_VIDEO_ITEM_H
#define NAGAMETV_VIDEO_ITEM_H

#include <QtCore/QCoreApplication>
#include <QtCore/QThread>
#include <QtQuick/QQuickItem>
#include <cstdint>

// Requires null or a live QQuickItem, with no concurrent destruction. The
// application and its GUI thread must remain alive throughout this call.
// This does not acquire ownership or make a dangling pointer safe.
inline std::uint8_t *qml6VideoItemPointer(QQuickItem *item) {
    auto *app = QCoreApplication::instance();
    if (!item || !app || QThread::currentThread() != app->thread()) return nullptr;
    if (item->thread() != app->thread()) return nullptr;

    // qml6glsink's widget setter casts gpointer to Qt6GLVideoItem* without
    // checking. The plugin's private C++ header is not a public dependency.
    // Qt's meta-cast checks the inheritance chain (including QML subclasses)
    // and returns the actual base pointer, not just a class-name match.
    // The application loads the genuine GStreamer QML type; native plugins
    // must not impersonate this class in their Qt meta-object.
    // See docs/video-item-safety.md for the upstream contracts.
    return static_cast<std::uint8_t *>(item->qt_metacast("Qt6GLVideoItem"));
}

#endif
