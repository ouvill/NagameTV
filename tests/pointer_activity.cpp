#include "pointer_activity.h"
#include <QtGui/QGuiApplication>
#include <QtQml/QQmlComponent>
#include <QtQml/QQmlEngine>
#include <QtTest/QSignalSpy>
#include <QtTest/QTest>
#include <memory>
#include <cstdlib>

static void check(bool ok, const char *message) {
  if (!ok) { qCritical() << message; std::exit(1); }
}

class PressSink : public QQuickItem {
public:
  explicit PressSink(QQuickItem *parent) : QQuickItem(parent) {
    setAcceptedMouseButtons(Qt::LeftButton);
  }
  int presses = 0;
  void mousePressEvent(QMouseEvent *event) override { ++presses; event->accept(); }
};

static void move(QQuickWindow &window, QPointF position) {
  // Deliver a window event directly: no QML HoverHandler or platform hover state.
  QMouseEvent event(QEvent::MouseMove, position, position,
                    Qt::NoButton, Qt::NoButton, Qt::NoModifier);
  QCoreApplication::sendEvent(&window, &event);
}

int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  QQmlEngine engine;
  QQuickWindow first, second;
  first.resize(400, 300);
  second.resize(400, 300);
  QQmlComponent component(&engine);
  component.setData("import QtQuick\nItem { signal activity() }", QUrl());
  std::unique_ptr<QObject> object(component.create());
  check(object != nullptr, "Could not create the QML activity item");
  auto *item = qobject_cast<QQuickItem *>(object.get());
  check(item != nullptr, "Expected QQuickItem");
  item->setParentItem(first.contentItem());
  item->setPosition({40, 40});
  item->setSize({300, 200});
  item->setZ(402);
  QSignalSpy activity(item, SIGNAL(activity()));
  installPointerActivity(item);
  installPointerActivity(item);
  move(first, {80, 80});
  check(activity.size() == 1, "Move must signal once, even after repeated installation");
  move(first, {80, 80});
  check(activity.size() == 1, "Same position must not postpone auto-hide");
  move(first, {90, 80});
  check(activity.size() == 2, "Continued movement must postpone auto-hide");
  move(first, {10, 10});
  check(activity.size() == 2, "Outside the video must not signal");
  move(first, {90, 80});
  check(activity.size() == 3, "Reentry at the previous position must signal");
  item->setVisible(false);
  move(first, {100, 80});
  check(activity.size() == 3, "Hidden activity item must not signal");
  item->setVisible(true);
  item->setParentItem(second.contentItem());
  move(first, {110, 80});
  check(activity.size() == 3, "Previous window must no longer be observed");
  move(second, {110, 80});
  check(activity.size() == 4, "Observer must follow its item's window");

  PressSink sink(second.contentItem());
  sink.setSize({400, 300});
  second.show();
  check(QTest::qWaitForWindowExposed(&second), "Test window did not become exposed");
  QTest::mouseClick(&second, Qt::LeftButton, Qt::NoModifier, {150, 120});
  check(sink.presses == 1, "Observer must not consume clicks");
  const auto observer = QPointer<QObject>(item->children().last());
  object.reset();
  check(observer.isNull(), "Destroying the item must release the observer");
  move(second, {120, 80});
  qInfo() << "Pointer activity checks passed on" << QGuiApplication::platformName();
}
