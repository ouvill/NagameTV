#ifndef MINIMAL_VIEWER_PORTAL_H
#define MINIMAL_VIEWER_PORTAL_H

#include <QtCore/QtGlobal>
// Linux desktop portal probing. Keep routing and fallback policy in Rust.
#ifdef Q_OS_LINUX
#include <QtCore/QCoreApplication>
#include <QtCore/QDir>
#include <QtCore/QJsonArray>
#include <QtCore/QJsonObject>
#include <QtCore/QPluginLoader>
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusMessage>
#include <QtDBus/QDBusReply>
#include <QtDBus/QDBusVariant>
#include <stdexcept>

inline bool portalThemeLoaded() {
    for (const auto &path : QCoreApplication::libraryPaths()) {
        const QDir directory(path + QStringLiteral("/platformthemes"));
        for (const auto &file : directory.entryList(QDir::Files)) {
            QPluginLoader plugin(directory.absoluteFilePath(file));
            const auto keys = plugin.metaData().value(QStringLiteral("MetaData")).toObject()
                                 .value(QStringLiteral("Keys")).toArray();
            if (keys.contains(QStringLiteral("xdgdesktopportal")) && plugin.isLoaded())
                return true;
        }
    }
    return false;
}

inline unsigned int portalFileChooserVersion() {
    auto message = QDBusMessage::createMethodCall(
        QStringLiteral("org.freedesktop.portal.Desktop"),
        QStringLiteral("/org/freedesktop/portal/desktop"),
        QStringLiteral("org.freedesktop.DBus.Properties"), QStringLiteral("Get"));
    message << QStringLiteral("org.freedesktop.portal.FileChooser") << QStringLiteral("version");
    // A missing/unresponsive portal must not hold up native application startup
    // for the default 25-second D-Bus timeout. This call can activate the service.
    const QDBusReply<QDBusVariant> reply =
        QDBusConnection::sessionBus().call(message, QDBus::Block, 1000);
    if (!reply.isValid())
        throw std::runtime_error(reply.error().message().toStdString());
    const auto value = reply.value().variant();
    if (value.metaType().id() != QMetaType::UInt)
        throw std::runtime_error("FileChooser portal returned an invalid version");
    return value.toUInt();
}
#endif
#endif
