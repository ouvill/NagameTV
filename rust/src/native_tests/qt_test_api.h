#ifndef VIEWER_NATIVE_TESTS_QT_TEST_API_H
#define VIEWER_NATIVE_TESTS_QT_TEST_API_H
// Missing binding operations only. Test cases, inputs and assertions live in Rust.
#include "localization.h"
#include "pointer_activity.h"
#include <QtCore/QFile>
#include <QtCore/QMimeData>
#include <QtGui/QDragEnterEvent>
#include <QtGui/QDropEvent>
#include <QtGui/QImage>
#include <QtGui/QPainter>
#include <QtGui/QPainterPath>
#include <QtSvg/QSvgRenderer>
#include <QtQml/QQmlExpression>
#include <memory>
#include <stdexcept>

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
inline QVariant evaluateRoot(QQmlApplicationEngine &engine, const QString &source) {
    const auto roots = engine.rootObjects();
    if (roots.isEmpty()) throw std::runtime_error("QML root is missing");
    QQmlExpression expression(qmlContext(roots.first()), roots.first(), source);
    const auto result = expression.evaluate();
    if (expression.hasError())
        throw std::runtime_error(expression.error().toString().toStdString());
    return result;
}

inline bool dropFileOnRoot(QQmlApplicationEngine &engine, const QString &url, const QPoint &position) {
    const auto roots = engine.rootObjects();
    auto *window = roots.isEmpty() ? nullptr : qobject_cast<QQuickWindow *>(roots.first());
    if (!window) return false;
    QMimeData mime;
    mime.setUrls({QUrl(url)});
    QDragEnterEvent enter(position, Qt::CopyAction, &mime, Qt::LeftButton, Qt::NoModifier);
    QCoreApplication::sendEvent(window, &enter);
    if (!enter.isAccepted()) return false;
    QDropEvent drop(QPointF(position), Qt::CopyAction, &mime, Qt::LeftButton, Qt::NoModifier);
    QCoreApplication::sendEvent(window, &drop);
    return drop.isAccepted();
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
#endif
