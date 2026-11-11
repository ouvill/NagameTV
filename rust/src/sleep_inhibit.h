#ifndef NAGAMETV_SLEEP_INHIBIT_H
#define NAGAMETV_SLEEP_INHIBIT_H
#include <QtCore/QtGlobal>
#if defined(Q_OS_LINUX) || defined(Q_MOC_RUN)
#include <QtCore/QObject>
#include <QtCore/QElapsedTimer>
#include <QtCore/QUuid>
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusError>
#include <QtDBus/QDBusMessage>
#include <QtDBus/QDBusObjectPath>
#include <QtDBus/QDBusPendingReply>
#include <QtDBus/QDBusServiceWatcher>
#include <QtDBus/QDBusUnixFileDescriptor>
#include "sleep_inhibit_x11.h"
#include <cstdint>
#include <memory>
#include <stdexcept>

class InhibitEnvironment final {
    const QString name = QStringLiteral("nagametv_inhibit_environment_") + QUuid::createUuid().toString(QUuid::Id128);
    QDBusConnection session = QDBusConnection::connectToBus(QDBusConnection::SessionBus, name);
    std::unique_ptr<QDBusConnection> system;
    QDBusServiceWatcher sessionWatcher{QStringLiteral("org.freedesktop.portal.Desktop"), session,
                                      QDBusServiceWatcher::WatchForOwnerChange};
    std::unique_ptr<QDBusServiceWatcher> systemWatcher;
    std::uint64_t generation = 0;
    std::uint64_t portalGeneration = 0, screenGeneration = 0, systemGeneration = 0;
public:
    InhibitEnvironment() {
        sessionWatcher.addWatchedService(QStringLiteral("org.freedesktop.ScreenSaver"));
        QObject::connect(&sessionWatcher, &QDBusServiceWatcher::serviceOwnerChanged, &sessionWatcher,
                         [this](const QString &service, const QString &, const QString &) {
            ++generation;
            if (service == QStringLiteral("org.freedesktop.portal.Desktop")) ++portalGeneration;
            else ++screenGeneration;
        });
    }
    void observeSystem() {
        if (system) return;
        system = std::make_unique<QDBusConnection>(QDBusConnection::connectToBus(
            QDBusConnection::SystemBus, name + QStringLiteral("_system")));
        systemWatcher = std::make_unique<QDBusServiceWatcher>(QStringLiteral("org.freedesktop.login1"),
            *system, QDBusServiceWatcher::WatchForOwnerChange);
        QObject::connect(systemWatcher.get(), &QDBusServiceWatcher::serviceOwnerChanged,
                         systemWatcher.get(), [this] { ++generation; ++systemGeneration; });
    }
    std::uint64_t revision() const { return generation; }
    std::uint64_t revisionFor(const QString &kind) const {
        if (kind == QStringLiteral("portal")) return portalGeneration;
        if (kind == QStringLiteral("screensaver")) return screenGeneration;
        if (kind == QStringLiteral("logind")) return systemGeneration;
        return 0;
    }
    ~InhibitEnvironment() {
        QDBusConnection::disconnectFromBus(name);
        if (system) QDBusConnection::disconnectFromBus(name + QStringLiteral("_system"));
    }
};
inline std::unique_ptr<InhibitEnvironment> makeInhibitEnvironment() { return std::make_unique<InhibitEnvironment>(); }

// Wire operations only. Rust owns route selection, caching and viewing policy.
class SleepInhibit final : public QObject {
    Q_OBJECT
    static constexpr int CallTimeoutMs = 1500;
    static constexpr uint Suspend = 4;
    static constexpr uint Idle = 8;
    const QString kind;
    const QString service;
    const QString name = QStringLiteral("nagametv_inhibit_") + QUuid::createUuid().toString(QUuid::Id128);
    std::unique_ptr<QDBusConnection> bus;
    std::unique_ptr<QDBusServiceWatcher> watcher;
    std::unique_ptr<QDBusPendingCall> pending;
    std::unique_ptr<X11SleepInhibit> x11;
    QDBusUnixFileDescriptor lock;
    uint cookie = 0;
    QString path;
    QString failure;
    QElapsedTimer elapsed;
    bool accepted = false;
    bool replied = false;
    enum class State { Pending, Held, Unavailable, Failed, Closed };
    State state = State::Pending;

    void fail(const QDBusError &error) {
        failure = error.name() + QStringLiteral(": ") + error.message();
        switch (error.type()) {
        case QDBusError::ServiceUnknown: case QDBusError::UnknownInterface:
        case QDBusError::UnknownMethod: case QDBusError::UnknownObject:
        case QDBusError::Disconnected: case QDBusError::NoServer:
            state = State::Unavailable; break;
        default: state = State::Failed; break;
        }
    }
private Q_SLOTS:
    void response(uint code, const QVariantMap &) {
        if (state != State::Pending) return;
        if (code == 0) accepted = true;
        else { failure = QStringLiteral("Inhibit request Response rejected with code %1").arg(code); state = State::Failed; }
    }
public:
    SleepInhibit(const QString &backend, const QString &reason)
        : kind(backend), service(backend == QStringLiteral("portal") ? QStringLiteral("org.freedesktop.portal.Desktop")
            : backend == QStringLiteral("logind") ? QStringLiteral("org.freedesktop.login1") : QStringLiteral("org.freedesktop.ScreenSaver")) {
        if (kind == QStringLiteral("x11")) {
            try { x11 = X11SleepInhibit::create(); state = State::Held; }
            catch (const std::exception &error) { failure = QString::fromUtf8(error.what()); state = State::Unavailable; }
            return;
        }
        bus = std::make_unique<QDBusConnection>(QDBusConnection::connectToBus(
            kind == QStringLiteral("logind") ? QDBusConnection::SystemBus : QDBusConnection::SessionBus, name));
        if (!bus->isConnected()) { fail(bus->lastError()); state = State::Unavailable; return; }
        watcher = std::make_unique<QDBusServiceWatcher>(service, *bus, QDBusServiceWatcher::WatchForOwnerChange);
        QObject::connect(watcher.get(), &QDBusServiceWatcher::serviceOwnerChanged, watcher.get(),
                         [this](const QString &, const QString &oldOwner, const QString &) {
            if (!oldOwner.isEmpty() && state != State::Closed) { failure = QStringLiteral("inhibitor service owner disappeared or changed"); state = State::Failed; }
        });
        auto message = QDBusMessage::createMethodCall(service,
            kind == QStringLiteral("portal") ? QStringLiteral("/org/freedesktop/portal/desktop")
                : kind == QStringLiteral("logind") ? QStringLiteral("/org/freedesktop/login1") : QStringLiteral("/org/freedesktop/ScreenSaver"),
            kind == QStringLiteral("portal") ? QStringLiteral("org.freedesktop.portal.Inhibit")
                : kind == QStringLiteral("logind") ? QStringLiteral("org.freedesktop.login1.Manager") : service,
            QStringLiteral("Inhibit"));
        if (kind == QStringLiteral("portal")) {
            const auto token = QStringLiteral("nagametv_") + QUuid::createUuid().toString(QUuid::Id128);
            auto sender = bus->baseService().mid(1); sender.replace(QLatin1Char('.'), QLatin1Char('_'));
            path = QStringLiteral("/org/freedesktop/portal/desktop/request/") + sender + QLatin1Char('/') + token;
            if (!bus->connect(service, path, QStringLiteral("org.freedesktop.portal.Request"), QStringLiteral("Response"),
                              this, SLOT(response(uint,QVariantMap)))) { fail(bus->lastError()); return; }
            message << QString() << uint(Suspend | Idle)
                    << QVariantMap{{QStringLiteral("handle_token"), token}, {QStringLiteral("reason"), reason}};
        } else if (kind == QStringLiteral("logind")) {
            message << QStringLiteral("sleep:idle") << QStringLiteral("NagameTV") << reason << QStringLiteral("block");
        } else { message << QStringLiteral("NagameTV") << reason; }
        elapsed.start();
        pending = std::make_unique<QDBusPendingCall>(bus->asyncCall(message, CallTimeoutMs));
    }
    void release() {
        if (state == State::Closed) return;
        const bool held = state == State::Held;
        state = State::Closed;
        lock = QDBusUnixFileDescriptor();
        if (x11) { x11->release(); x11.reset(); }
        if (!bus) return;
        auto message = QDBusMessage::createMethodCall(service,
            kind == QStringLiteral("portal") ? path : QStringLiteral("/org/freedesktop/ScreenSaver"),
            kind == QStringLiteral("portal") ? QStringLiteral("org.freedesktop.portal.Request") : service,
            kind == QStringLiteral("portal") ? QStringLiteral("Close") : QStringLiteral("UnInhibit"));
        if (kind == QStringLiteral("screensaver") && held) message << cookie;
        else if (kind != QStringLiteral("portal") || path.isEmpty()) return;
        message.setAutoStartService(false);
        // Disconnection releases cookies and late grants even if remote cleanup fails.
        if (!bus->send(message)) throw std::runtime_error(bus->lastError().message().toStdString());
    }
    ~SleepInhibit() { if (bus) QDBusConnection::disconnectFromBus(name); }
    int poll() {
        if (bus && !bus->isConnected() && state != State::Closed) { fail(bus->lastError()); state = State::Unavailable; }
        if (state == State::Pending && pending && pending->isFinished()) {
            if (kind == QStringLiteral("portal")) {
                QDBusPendingReply<QDBusObjectPath> reply(*pending);
                if (reply.isError()) fail(reply.error());
                else {
                    const auto returned = reply.value().path();
                    if (!returned.startsWith(QStringLiteral("/org/freedesktop/portal/desktop/request/"))) {
                        failure = QStringLiteral("Inhibit returned an invalid request path"); state = State::Failed;
                    } else {
                        if (returned != path) {
                            accepted = false;
                            bus->disconnect(service, path, QStringLiteral("org.freedesktop.portal.Request"), QStringLiteral("Response"), this, SLOT(response(uint,QVariantMap)));
                            path = returned;
                            if (!bus->connect(service, path, QStringLiteral("org.freedesktop.portal.Request"), QStringLiteral("Response"), this, SLOT(response(uint,QVariantMap)))) fail(bus->lastError());
                        }
                        replied = true;
                    }
                }
            } else if (kind == QStringLiteral("logind")) {
                QDBusPendingReply<QDBusUnixFileDescriptor> reply(*pending);
                if (reply.isError()) fail(reply.error());
                else if (!reply.value().isValid()) { failure = QStringLiteral("logind returned an invalid inhibitor FD"); state = State::Failed; }
                else { lock = reply.value(); state = State::Held; }
            } else {
                QDBusPendingReply<uint> reply(*pending);
                if (reply.isError()) fail(reply.error());
                else { cookie = reply.value(); state = State::Held; }
            }
            pending.reset();
        }
        if (state == State::Pending && replied && accepted) state = State::Held;
        if (state == State::Pending && elapsed.isValid() && elapsed.elapsed() >= CallTimeoutMs) {
            failure = QStringLiteral("inhibitor did not confirm acquisition before the deadline"); state = State::Failed;
        }
        switch (state) { case State::Pending: return 0; case State::Held: return 1;
            case State::Unavailable: return 2; case State::Failed: return 3; case State::Closed: return 4; }
        return 3;
    }
    QString error() const { return failure; }
};
inline std::unique_ptr<SleepInhibit> acquireSleepInhibit(const QString &backend, const QString &reason) {
    return std::make_unique<SleepInhibit>(backend, reason);
}
inline void checkX11SleepInhibit() {
    // Explicit wire check on the validated private Xwayland; the product factory
    // never claims that this can inhibit the parent Wayland compositor.
    auto lease = X11SleepInhibit::create(true);
    lease->release();
    lease->release();
}
#endif
#endif
