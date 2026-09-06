#ifndef MIRAKURUN_VIEWER_POINTER_ACTIVITY_H
#define MIRAKURUN_VIEWER_POINTER_ACTIVITY_H

#include <QtCore/QPointer>
#include <QtGui/QEnterEvent>
#include <QtGui/QMouseEvent>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>
#include <optional>

// Observe the window before Qt Quick's hover delivery. In particular, this
// does not depend on HoverHandler receiving events from the Wayland backend.
// The item owns the observer; Qt removes its event filter when it is destroyed.
class ViewerPointerActivity final : public QObject {
public:
  explicit ViewerPointerActivity(QQuickItem *item) : QObject(item), item_(item) {
    connect(item, &QQuickItem::windowChanged, this,
            [this](QQuickWindow *window) { attach(window); });
    attach(item->window());
  }

protected:
  bool eventFilter(QObject *watched, QEvent *event) override {
    if (watched != window_) return false;
    switch (event->type()) {
    case QEvent::MouseMove:
      record(static_cast<QMouseEvent *>(event)->position());
      break;
    case QEvent::Enter:
      lastPosition_.reset();
      record(static_cast<QEnterEvent *>(event)->position());
      break;
    case QEvent::Leave:
      lastPosition_.reset();
      break;
    default:
      break;
    }
    return false; // Observe only: clicks, dragging and hover still reach controls.
  }

private:
  void attach(QQuickWindow *window) {
    if (window_) window_->removeEventFilter(this);
    window_ = window;
    lastPosition_.reset();
    if (window_) window_->installEventFilter(this);
  }

  void record(const QPointF &position) {
    const bool inside = item_->isVisible() && item_->isEnabled()
        && item_->contains(item_->mapFromScene(position));
    if (!inside) {
      lastPosition_.reset();
      return;
    }
    if (lastPosition_ && *lastPosition_ == position) return;
    lastPosition_ = position;
    // Both objects live on the GUI thread. The QML component declares activity().
    QMetaObject::invokeMethod(item_, "activity", Qt::DirectConnection);
  }

  QQuickItem *item_;
  QPointer<QQuickWindow> window_;
  std::optional<QPointF> lastPosition_;
};

inline void installPointerActivity(QQuickItem *item) {
  if (!item) return;
  // Reattachment must not accumulate observers or duplicate signal delivery.
  for (auto *child : item->children())
    if (dynamic_cast<ViewerPointerActivity *>(child)) return;
  new ViewerPointerActivity(item);
}

#endif
