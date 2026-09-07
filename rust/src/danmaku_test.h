#pragma once
#include <QtCore/QString>
#include <QtQuickTest/quicktest.h>

// The module is registered by Rust before creating the real Qt Quick test app.
inline int run_qml_tests(const QString &path) {
    char executable[] = "viewer-qml-tests";
    char input[] = "-input";
    QByteArray directory = path.toUtf8();
    char *argv[] = {executable, input, directory.data(), nullptr};
    return quick_test_main(3, argv, "viewer", nullptr);
}
