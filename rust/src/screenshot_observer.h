#ifndef VIEWER_SCREENSHOT_OBSERVER_H
#define VIEWER_SCREENSHOT_OBSERVER_H
#include "rust/cxx.h"
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>
#include <memory>
namespace viewer_screenshot {

// These callbacks only retain immutable values in a Rust mutex; they never
// execute QML, touch GUI objects or download pixels on the rendering thread.
template<typename Observer>
inline void observePresentation(QQuickItem *item, rust::Box<Observer> observer) {
    auto owned = std::make_shared<rust::Box<Observer>>(std::move(observer));
    auto *context = new QObject(item);
    auto attach = [context, owned](QQuickWindow *window) {
        if (!window) return;
        QObject::connect(window, &QQuickWindow::beforeSynchronizing, context,
            [owned]() { (*owned)->before_sync(); }, Qt::DirectConnection);
        QObject::connect(window, &QQuickWindow::afterSynchronizing, context,
            [owned]() { (*owned)->after_sync(); }, Qt::DirectConnection);
        QObject::connect(window, &QQuickWindow::frameSwapped, context,
            [owned]() { (*owned)->presented(); }, Qt::DirectConnection);
        QObject::connect(window, &QQuickWindow::sceneGraphInvalidated, context,
            [owned]() { (*owned)->invalidated(); }, Qt::DirectConnection);
    };
    if (item->window()) attach(item->window());
    else QObject::connect(item, &QQuickItem::windowChanged, context, attach, Qt::SingleShotConnection);
}
}
#endif
