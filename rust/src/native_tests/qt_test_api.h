#ifndef VIEWER_NATIVE_TESTS_QT_TEST_API_H
#define VIEWER_NATIVE_TESTS_QT_TEST_API_H
// Missing binding operations only. Test cases, inputs and assertions live in Rust.
#include "localization.h"
#include "pointer_activity.h"
#include "danmaku_test.h"
#include "rust/cxx.h"
#include <QtCore/QFile>
#include <QtCore/QMimeData>
#include <QtCore/QElapsedTimer>
#include <QtGui/QDragEnterEvent>
#include <QtGui/QDropEvent>
#include <QtGui/QImage>
#include <QtGui/QPainter>
#include <QtGui/QPainterPath>
#include <QtSvg/QSvgRenderer>
#include <QtQml/QQmlExpression>
#include <QtTest/qtestkeyboard.h>
#include <QtTest/qtestmouse.h>
#include <memory>
#include <stdexcept>
#include <mutex>
#include <vector>

inline QImage grabRoot(QQmlApplicationEngine &engine) {
    const auto roots = engine.rootObjects();
    auto *window = roots.isEmpty() ? nullptr : qobject_cast<QQuickWindow *>(roots.first());
    if (!window) throw std::runtime_error("QQuickWindow is missing");
    return window->grabWindow();
}
inline void clickRootKey(QQmlApplicationEngine &engine, const QString &sequence) {
    const auto roots = engine.rootObjects();
    auto *window = roots.isEmpty() ? nullptr : qobject_cast<QQuickWindow *>(roots.first());
    if (!window) throw std::runtime_error("QQuickWindow is missing");
    const QKeySequence keys(sequence, QKeySequence::PortableText);
    if (keys.count() != 1) throw std::runtime_error("One key combination is required");
    QTest::keyClick(window, keys[0].key(), keys[0].keyboardModifiers());
}
inline bool forwardFocusKey(const QString &sequence) {
    const QKeySequence keys(sequence, QKeySequence::PortableText);
    if (keys.count() != 1) throw std::runtime_error("One key combination is required");
    return sendTestForwardedKey(keys[0].key(), int(keys[0].keyboardModifiers()), QString(), false);
}
inline void doubleClickRoot(QQmlApplicationEngine &engine, const QPoint &position) {
    const auto roots = engine.rootObjects();
    auto *window = roots.isEmpty() ? nullptr : qobject_cast<QQuickWindow *>(roots.first());
    if (!window) throw std::runtime_error("QQuickWindow is missing");
    QTest::mouseDClick(window, Qt::LeftButton, Qt::NoModifier, position);
}

inline void clickRootItem(QQmlApplicationEngine &engine, const QString &name) {
    const auto roots = engine.rootObjects();
    auto *window = roots.isEmpty() ? nullptr : qobject_cast<QQuickWindow *>(roots.first());
    auto *item = window ? window->findChild<QQuickItem *>(name) : nullptr;
    if (!item) throw std::runtime_error("QQuickItem is missing");
    QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier,
                     item->mapToScene(QPointF(item->width() / 2, item->height() / 2)).toPoint());
}
// This object lives only in the native test runner. Observe frameSwapped on
// the render thread; GUI polling must not distort the measured intervals.
class FrameTimes : public QObject {
    struct State {
        std::mutex mutex;
        QElapsedTimer clock;
        std::vector<double> intervals;
    };
    std::shared_ptr<State> state = std::make_shared<State>();
public:
    explicit FrameTimes(QQuickWindow *window) {
        QObject::connect(window, &QQuickWindow::frameSwapped, this, [state = state] {
            std::lock_guard lock(state->mutex);
            constexpr size_t MaxSamples = 4096;
            if (state->clock.isValid() && state->intervals.size() < MaxSamples)
                state->intervals.push_back(double(state->clock.nsecsElapsed()) / 1000000.0);
            state->clock.start();
        }, Qt::DirectConnection);
    }
    QString samples() const {
        std::lock_guard lock(state->mutex);
        QStringList values;
        for (auto value : state->intervals) values.append(QString::number(value));
        return "[" + values.join(',') + "]";
    }
};
inline std::unique_ptr<FrameTimes> watchFrames(const QQmlApplicationEngine &engine) {
    const auto roots = engine.rootObjects();
    auto *window = roots.isEmpty() ? nullptr : qobject_cast<QQuickWindow *>(roots.first());
    if (!window) throw std::runtime_error("QQuickWindow is missing");
    return std::make_unique<FrameTimes>(window);
}

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

inline bool dropMimeOnRoot(QQmlApplicationEngine &engine, const QMimeData &mime, const QPoint &position) {
    const auto roots = engine.rootObjects();
    auto *window = roots.isEmpty() ? nullptr : qobject_cast<QQuickWindow *>(roots.first());
    if (!window) return false;
    QDragEnterEvent enter(position, Qt::CopyAction, &mime, Qt::LeftButton, Qt::NoModifier);
    QCoreApplication::sendEvent(window, &enter);
    if (!enter.isAccepted()) return false;
    QDropEvent drop(QPointF(position), Qt::CopyAction, &mime, Qt::LeftButton, Qt::NoModifier);
    QCoreApplication::sendEvent(window, &drop);
    return drop.isAccepted();
}

inline bool dropFileOnRoot(QQmlApplicationEngine &engine, const QString &url, const QPoint &position) {
    QMimeData mime;
    mime.setUrls({QUrl(url)});
    return dropMimeOnRoot(engine, mime, position);
}

inline bool dropFilesOnRoot(QQmlApplicationEngine &engine, rust::Slice<const rust::String> urls,
                            const QPoint &position) {
    QMimeData mime;
    QList<QUrl> files;
    for (const auto &url : urls)
        files.append(QUrl(QString::fromUtf8(url.data(), url.size())));
    mime.setUrls(files);
    return dropMimeOnRoot(engine, mime, position);
}

inline bool dropTransferOnRoot(QQmlApplicationEngine &engine, const QString &format,
                               const QByteArray &key, const QString &hostUrl, const QPoint &position) {
    QMimeData mime;
    mime.setData(format, key);
    if (!hostUrl.isEmpty()) mime.setUrls({QUrl(hostUrl)});
    return dropMimeOnRoot(engine, mime, position);
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
inline void sendPointerEnter(QQuickWindow &window, const QPointF &position) {
    QEnterEvent event(position, position, position);
    QCoreApplication::sendEvent(&window, &event);
}
inline void sendPointerLeave(QQuickWindow &window) {
    QEvent event(QEvent::Leave);
    QCoreApplication::sendEvent(&window, &event);
}
#endif
