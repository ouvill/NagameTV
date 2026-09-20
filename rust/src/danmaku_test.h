#ifndef VIEWER_DANMAKU_TEST_H
#define VIEWER_DANMAKU_TEST_H
#include <QtCore/QString>
#include <QtGui/QGuiApplication>
#include <QtGui/QInputMethodEvent>
#include <QtGui/QKeyEvent>
#include <QtQuickTest/quicktest.h>
#include "rust/cxx.h"
#include <vector>

// Qt Quick Test requires mutable argv storage for the duration of the call.
inline int run_qml_test_args(rust::Slice<const rust::String> arguments) {
    std::vector<QByteArray> storage;
    storage.reserve(arguments.size());
    for (const auto &argument : arguments)
        storage.emplace_back(argument.data(), argument.size());
    std::vector<char *> argv;
    for (auto &argument : storage) argv.push_back(argument.data());
    argv.push_back(nullptr);
    return quick_test_main(static_cast<int>(storage.size()), argv.data(), "viewer", nullptr);
}

// The module is registered by Rust before creating the real Qt Quick test app.
inline int run_qml_tests(const QString &path) {
    char executable[] = "viewer-qml-tests";
    char input[] = "-input";
    QByteArray directory = path.toUtf8();
    char *argv[] = {executable, input, directory.data(), nullptr};
    return quick_test_main(3, argv, "viewer", nullptr);
}

// Event delivery only; test cases and assertions remain in QML.
inline bool sendTestInputMethod(const QString &preedit, const QString &commit) {
    auto *target = QGuiApplication::focusObject();
    if (!target) return false;
    QInputMethodEvent event(preedit, {});
    event.setCommitString(commit);
    return QCoreApplication::sendEvent(target, &event);
}

inline bool sendTestInputMethodCursor(const QString &preedit) {
    auto *target = QGuiApplication::focusObject();
    if (!target) return false;
    const QInputMethodEvent::Attribute cursor(QInputMethodEvent::Cursor, preedit.size(), 1, QVariant());
    QInputMethodEvent event(preedit, {cursor});
    return QCoreApplication::sendEvent(target, &event);
}

// Match IBus's direct focus-object delivery instead of QTest's window path.
inline bool sendTestForwardedKey(int key, int modifiers, const QString &text, bool repeat) {
    auto *target = QGuiApplication::focusObject();
    if (!target) return false;
    QKeyEvent event(QEvent::KeyPress, key, Qt::KeyboardModifiers(modifiers), text, repeat);
    return QCoreApplication::sendEvent(target, &event);
}

#endif
