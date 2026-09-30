#ifndef NAGAMETV_BACK_BUTTON_H
#define NAGAMETV_BACK_BUTTON_H

#include <QtCore/QCoreApplication>
#include <QtCore/QPointer>
#include <QtGui/QKeyEvent>
#include <QtGui/QMouseEvent>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>

// Translate before Quick delivers the click to controls or popup overlays.
// Escape then follows the existing focused-popup and shortcut handling.
// The binding item owns the filter, including reattachment and destruction.
class ViewerBackButton final : public QObject {
public:
  explicit ViewerBackButton(QQuickItem *item) : QObject(item), item_(item) {
    connect(item, &QQuickItem::windowChanged, this,
            [this](QQuickWindow *window) { attach(window); });
    attach(item->window());
  }

protected:
  bool eventFilter(QObject *watched, QEvent *event) override {
    if (watched != window_ || !item_->isEnabled() || !item_->isVisible()
        || !window_->isActive()) return false;
    switch (event->type()) {
    case QEvent::KeyPress:
    case QEvent::KeyRelease: {
      const auto *key = static_cast<QKeyEvent *>(event);
      // A held Escape must not close a second popup after the first disappears.
      if (key->key() == Qt::Key_Escape && key->isAutoRepeat()) {
        event->accept();
        return true;
      }
      if (key->key() != Qt::Key_Back) return false;
      event->accept();
      if (event->type() == QEvent::KeyPress && !key->isAutoRepeat()) {
        const QPointer<QQuickWindow> target = window_;
        sendEscape(target->activeFocusItem(), QEvent::KeyPress);
        if (target) sendEscape(target->activeFocusItem(), QEvent::KeyRelease);
      }
      return true;
    }
    case QEvent::MouseButtonPress:
    case QEvent::MouseButtonDblClick:
    case QEvent::MouseButtonRelease:
      break;
    default:
      return false;
    }
    if (static_cast<QMouseEvent *>(event)->button() != Qt::BackButton)
      return false;
    event->accept();
    if (event->type() == QEvent::MouseButtonPress) {
      // Qt sends a second press before its double-click notification.
      // Only presses dismiss; releases and double clicks are consumed above.
      // Mouse modifiers do not change the meaning of the Back button.
      const QPointer<QQuickWindow> target = window_;
      sendEscape(target->activeFocusItem(), QEvent::KeyPress);
      if (target) sendEscape(target->activeFocusItem(), QEvent::KeyRelease);
    }
    return true;
  }

private:
  static void sendEscape(QQuickItem *focusItem, QEvent::Type type) {
    // Use the same focus-item path as IME-forwarded keys. Sending a synthetic
    // press to QQuickWindow can activate its shortcut map AND Keys.pressed.
    // Propagate unhandled keys so an editor inside a popup can close its popup.
    QKeyEvent key(type, Qt::Key_Escape, Qt::NoModifier);
    QPointer<QQuickItem> target = focusItem;
    while (target) {
      key.accept();
      QCoreApplication::sendEvent(target, &key);
      if (!target || key.isAccepted()) break;
      target = target->parentItem();
    }
  }

  void attach(QQuickWindow *window) {
    if (window_) window_->removeEventFilter(this);
    window_ = window;
    if (window_) window_->installEventFilter(this);
  }

  QQuickItem *item_;
  QPointer<QQuickWindow> window_;
};

inline void installBackButton(QQuickItem *item) {
  if (!item) return;
  for (auto *child : item->children())
    if (dynamic_cast<ViewerBackButton *>(child)) return;
  new ViewerBackButton(item);
}

#endif
