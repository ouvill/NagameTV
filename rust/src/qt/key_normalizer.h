#ifndef NAGAMETV_QT_KEY_NORMALIZER_H
#define NAGAMETV_QT_KEY_NORMALIZER_H

#include <QtCore/QtGlobal>

#ifdef Q_OS_LINUX
#include <QtCore/QScopedValueRollback>
#include <QtGui/QGuiApplication>
#include <QtGui/QKeyEvent>

namespace viewer::keys {

// XF86 keysyms from xkbcommon-keysyms.h. Keeping the protocol values here
// avoids requiring XKB development headers in builds that use the Qt SDK.
constexpr quint32 Xf86Ok = 0x10081160;
constexpr quint32 Xf86Close = 0x1008ff56;
constexpr quint32 Xf86ChannelUp = 0x10081192;
constexpr quint32 Xf86ChannelDown = 0x10081193;
constexpr quint32 Xf86Numeric0 = 0x10081200;
constexpr quint32 Xf86Numeric9 = 0x10081209;

constexpr int normalizedKey(int key, quint32 keysym) {
    switch (keysym) {
    case Xf86Ok: return Qt::Key_Return;
    case Xf86Close: return Qt::Key_Back;
    case Xf86ChannelUp: return Qt::Key_PageUp;
    case Xf86ChannelDown: return Qt::Key_PageDown;
    default:
        if (keysym >= Xf86Numeric0 && keysym <= Xf86Numeric9)
            return Qt::Key_0 + int(keysym - Xf86Numeric0);
        return key;
    }
}

class KeyNormalizer final : public QObject {
public:
    explicit KeyNormalizer(QGuiApplication &app) : QObject(&app) {
        app.installEventFilter(this);
    }

protected:
    bool eventFilter(QObject *watched, QEvent *event) override {
        const auto type = event->type();
        if (type != QEvent::KeyPress && type != QEvent::KeyRelease
            && type != QEvent::ShortcutOverride)
            return false;

        auto &key = *static_cast<QKeyEvent *>(event);
        // Wayland and X11 report the resolved XKB symbol here. Scan codes
        // depend on the platform and don't account for the active keymap.
        const int mapped = normalizedKey(key.key(), key.nativeVirtualKey());
        if (type == QEvent::ShortcutOverride) {
            // Qt Quick checks shortcuts for synthetic presses and also sends
            // Keys.pressed. Use the shared focus-item dispatch for the replay
            // so a channel or digit binding cannot run a second time.
            const bool replay = replacement_ && key.nativeVirtualKey() == 0
                && key.key() == replacement_->key() && key.timestamp() == replacement_->timestamp();
            if (mapped == key.key() && !replay) return false;
            key.accept();
            return true;
        }
        if (mapped == key.key()) return false;

        QKeyEvent replacement(type, mapped, key.modifiers(),
                              key.nativeScanCode(), key.nativeVirtualKey(), key.nativeModifiers(),
                              key.text(), key.isAutoRepeat(), key.count(), key.device());
        replacement.setTimestamp(key.timestamp());
        replacement.setAccepted(key.isAccepted());
        QScopedValueRollback<const QKeyEvent *> replay(replacement_, &replacement);
        QCoreApplication::sendEvent(watched, &replacement);
        key.setAccepted(replacement.isAccepted());
        return true;
    }

private:
    const QKeyEvent *replacement_ = nullptr;
};

} // namespace viewer::keys

inline void installKeyNormalizer(QGuiApplication &app) {
    new viewer::keys::KeyNormalizer(app);
}
#endif

#endif
