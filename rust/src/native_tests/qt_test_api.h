#pragma once
// Missing binding operations only. Test cases, inputs and assertions live in Rust.
#include "localization.h"
#include "pointer_activity.h"
#include <QtCore/QFile>
#include <QtGui/QImage>
#include <QtGui/QPainter>
#include <QtGui/QPainterPath>
#include <QtSvg/QSvgRenderer>
#include <memory>

inline void disableTranslationCatalog() { Q_CLEANUP_RESOURCE(translations_qrc); }
inline void enableTranslationCatalog() { Q_INIT_RESOURCE(translations_qrc); }
inline bool translationCatalogExists() { return QFile::exists(":/i18n/ja.qm"); }
inline int translatorCount() {
    return QCoreApplication::instance()->findChildren<QTranslator *>().size();
}
inline void setDefaultLocale(const QString &name) { QLocale::setDefault(QLocale(name)); }
inline int rootCount(const QQmlApplicationEngine &engine) { return engine.rootObjects().size(); }
inline QVariant rootProperty(const QQmlApplicationEngine &engine, const QString &name) {
    const auto roots = engine.rootObjects();
    return roots.isEmpty() ? QVariant() : roots.first()->property(name.toUtf8().constData());
}
inline bool setRootProperty(QQmlApplicationEngine &engine, const QString &name, const QVariant &value) {
    const auto roots = engine.rootObjects();
    return !roots.isEmpty() && roots.first()->setProperty(name.toUtf8().constData(), value);
}

// QPainter retains QPaintDevice*. Bound it to the image borrow entirely in C++.
inline void rasterPath(const QPainterPath &path, QImage &image, const QPoint &offset) {
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.translate(offset);
    painter.setPen(Qt::NoPen);
    painter.setBrush(Qt::white);
    painter.drawPath(path);
}
inline void rasterSvg(QSvgRenderer &renderer, QImage &image) {
    QPainter painter(&image);
    renderer.render(&painter);
}

using ObserverLifetime = QPointer<QObject>;
inline int pointerObserverCount(const QQuickItem &item) {
    int count = 0;
    for (auto *child : item.children())
        if (dynamic_cast<ViewerPointerActivity *>(child)) ++count;
    return count;
}
inline std::unique_ptr<ObserverLifetime> watchPointerObserver(const QQuickItem &item) {
    for (auto *child : item.children())
        if (dynamic_cast<ViewerPointerActivity *>(child))
            return std::make_unique<ObserverLifetime>(child);
    return std::make_unique<ObserverLifetime>();
}
inline void sendMouseMove(QQuickWindow &window, const QPointF &position) {
    QMouseEvent event(QEvent::MouseMove, position, position,
                     Qt::NoButton, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(&window, &event);
}
