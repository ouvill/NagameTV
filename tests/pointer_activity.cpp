#include "pointer_activity.h"
#include <QtTest/QSignalSpy>
#include <QtTest/QTest>

class ActivityItem final : public QQuickItem {
    Q_OBJECT
public:
    using QQuickItem::QQuickItem;
signals:
    void activity();
};

class PointerActivityTest final : public QObject {
    Q_OBJECT
private slots:
    void observation_lifetime_and_window_changes() {
        QQuickWindow first;
        QQuickWindow second;
        auto *item = new ActivityItem(first.contentItem());
        item->setSize(QSizeF(100, 100));
        installPointerActivity(item);
        installPointerActivity(item);
        ViewerPointerActivity *observer = nullptr;
        int observers = 0;
        for (auto *child : item->children()) {
            if (auto *candidate = dynamic_cast<ViewerPointerActivity *>(child)) {
                observer = candidate;
                ++observers;
            }
        }
        QCOMPARE(observers, 1);
        QPointer<QObject> lifetime(observer);
        QSignalSpy activity(item, &ActivityItem::activity);
        auto move = [](QQuickWindow &window, QPointF position) {
            QMouseEvent event(QEvent::MouseMove, position, position,
                              Qt::NoButton, Qt::NoButton, Qt::NoModifier);
            QCoreApplication::sendEvent(&window, &event);
        };
        // Native observation works without consuming the Qt Quick input path.
        move(first, {20, 20});
        QCOMPARE(activity.count(), 1);
        move(first, {20, 20});
        QCOMPARE(activity.count(), 1);
        item->setEnabled(false);
        move(first, {25, 25});
        QCOMPARE(activity.count(), 1);
        item->setEnabled(true);
        item->setParentItem(second.contentItem());
        move(first, {30, 30});
        QCOMPARE(activity.count(), 1);
        move(second, {30, 30});
        QCOMPARE(activity.count(), 2);
        delete item;
        QVERIFY(lifetime.isNull());
        // Qt must have removed the dead event filter from the second window.
        move(second, {40, 40});
    }
};

QTEST_MAIN(PointerActivityTest)
#include "pointer_activity.moc"
