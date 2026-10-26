#ifndef NAGAMETV_REMOTE_KEY_H
#define NAGAMETV_REMOTE_KEY_H

#include <QtCore/QCoreApplication>
#include <QtGui/QKeyEvent>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>

inline bool supportsNavigationKey(int key) {
  switch (key) {
  case Qt::Key_Up:
  case Qt::Key_Down:
  case Qt::Key_Left:
  case Qt::Key_Right:
  case Qt::Key_Return:
  case Qt::Key_Back:
    return true;
  default:
    return false;
  }
}

inline bool postNavigationKey(QQuickItem *item, int key) {
  if (!supportsNavigationKey(key) || !item || !item->window()) return false;
  auto *window = item->window();
  // Remote input must not steal activation from another desktop application.
  // Qt owns these events and discards them if the window is destroyed first.
  QCoreApplication::postEvent(window, new QKeyEvent(QEvent::KeyPress, key, Qt::NoModifier, QString(), false));
  QCoreApplication::postEvent(window, new QKeyEvent(QEvent::KeyRelease, key, Qt::NoModifier, QString(), false));
  return true;
}

#endif
