#ifndef NAGAMETV_SCREEN_REFRESH_H
#define NAGAMETV_SCREEN_REFRESH_H
#include "rust/cxx.h"
#include <QtGui/QScreen>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>
#include <memory>

// Called for a validated GUI-thread item. All callbacks run on that thread,
// and destroying the item disconnects them before releasing the Rust observer.
template<typename Observer>
inline void observeScreenRefresh(QQuickItem *item, rust::Box<Observer> observer) {
    auto owned = std::make_shared<rust::Box<Observer>>(std::move(observer));
    auto *context = new QObject(item);
    auto rateConnection = std::make_shared<QMetaObject::Connection>();
    auto screenConnection = std::make_shared<QMetaObject::Connection>();
    auto screenChanged = [context, owned, rateConnection](QScreen *screen) {
        QObject::disconnect(*rateConnection);
        // A detached item has no presentation delay; attachment will report
        // the real screen. Never substitute a guessed refresh rate.
        (*owned)->changed(screen ? screen->refreshRate() : 0.0);
        if (screen) {
            *rateConnection = QObject::connect(screen, &QScreen::refreshRateChanged,
                context, [owned](qreal rate) { (*owned)->changed(rate); });
        }
    };
    auto windowChanged = [context, screenConnection, screenChanged](QQuickWindow *window) {
        QObject::disconnect(*screenConnection);
        screenChanged(window ? window->screen() : nullptr);
        if (window) {
            *screenConnection = QObject::connect(window, &QWindow::screenChanged,
                context, screenChanged);
        }
    };
    QObject::connect(item, &QQuickItem::windowChanged, context, windowChanged);
    windowChanged(item->window());
}
#endif
