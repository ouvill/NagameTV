#ifndef NAGAMETV_REMOTE_KEY_H
#define NAGAMETV_REMOTE_KEY_H

#include <QtCore/QCoreApplication>
#include <QtGui/QKeyEvent>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>

inline int cecDigitKey(unsigned char digit) { return digit <= 9 ? Qt::Key_0 + digit : 0; }
inline int cecSeekKey(bool forward) { return forward ? Qt::Key_Right : Qt::Key_Left; }
inline int cecChapterKey(bool forward) { return forward ? Qt::Key_MediaNext : Qt::Key_MediaPrevious; }
inline int cecBlueKey() { return Qt::Key_Blue; }
inline int cecRedKey() { return Qt::Key_Red; }
inline int cecGreenKey() { return Qt::Key_Green; }
inline int cecYellowKey() { return Qt::Key_Yellow; }

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
