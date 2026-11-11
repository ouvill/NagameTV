#ifndef NAGAMETV_SLEEP_INHIBIT_H
#define NAGAMETV_SLEEP_INHIBIT_H
#include <QtCore/QtGlobal>
// Linux portal wire operations only. Rust owns viewing policy and lease state.
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
#include <memory>
#include <stdexcept>

class SleepInhibit final : public QObject {
    Q_OBJECT
    static constexpr int CallTimeoutMs = 1500;
    static constexpr uint Suspend = 4;
    static constexpr uint Idle = 8;
    const QString service = QStringLiteral("org.freedesktop.portal.Desktop");
    const QString name = QStringLiteral("nagametv_inhibit_") + QUuid::createUuid().toString(QUuid::Id128);
    QDBusConnection bus = QDBusConnection::connectToBus(QDBusConnection::SessionBus, name);
    QDBusServiceWatcher watcher{service, bus, QDBusServiceWatcher::WatchForOwnerChange};
    std::unique_ptr<QDBusPendingCall> pending;
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
        case QDBusError::UnknownMethod: case QDBusError::Disconnected:
        case QDBusError::NoServer:
            state = State::Unavailable; break;
        default:
            state = State::Failed; break;
        }
    }
private Q_SLOTS:
    void response(uint code, const QVariantMap &) {
        if (state != State::Pending) return;
        if (code == 0) accepted = true;
        else {
            failure = QStringLiteral("Inhibit request Response rejected with code %1").arg(code);
            state = State::Failed;
        }
    }
public:
    explicit SleepInhibit(const QString &reason) {
        if (!bus.isConnected()) {
            fail(bus.lastError());
            state = State::Unavailable;
            return;
        }
        // A separate identity makes cancellation of a pending/late grant safe:
        // the portal also releases requests when their caller disappears.
        QObject::connect(&watcher, &QDBusServiceWatcher::serviceOwnerChanged, &watcher,
                         [this](const QString &, const QString &oldOwner, const QString &) {
            if (!oldOwner.isEmpty() && state != State::Closed) {
                failure = QStringLiteral("portal owner disappeared or changed");
                state = State::Failed;
            }
        });
        auto message = QDBusMessage::createMethodCall(service,
            QStringLiteral("/org/freedesktop/portal/desktop"),
            QStringLiteral("org.freedesktop.portal.Inhibit"), QStringLiteral("Inhibit"));
        const auto token = QStringLiteral("nagametv_") + QUuid::createUuid().toString(QUuid::Id128);
        auto sender = bus.baseService().mid(1);
        sender.replace(QLatin1Char('.'), QLatin1Char('_'));
        path = QStringLiteral("/org/freedesktop/portal/desktop/request/") + sender + QLatin1Char('/') + token;
        if (!bus.connect(service, path, QStringLiteral("org.freedesktop.portal.Request"),
                         QStringLiteral("Response"), this, SLOT(response(uint,QVariantMap)))) {
            fail(bus.lastError());
            return;
        }
        elapsed.start();
        message << QString() << uint(Suspend | Idle)
                << QVariantMap{{QStringLiteral("handle_token"), token}, {QStringLiteral("reason"), reason}};
        pending = std::make_unique<QDBusPendingCall>(bus.asyncCall(message, CallTimeoutMs));
    }
    // The caller consumes ownership after this nonblocking operation. Closing
    // the private connection also releases a grant if the remote Close fails.
    void release() {
        if (state == State::Closed) return;
        state = State::Closed;
        if (!path.isEmpty()) {
            auto message = QDBusMessage::createMethodCall(service, path,
                QStringLiteral("org.freedesktop.portal.Request"), QStringLiteral("Close"));
            message.setAutoStartService(false);
            // No reply is requested because disconnection is the final release
            // guarantee; do not retain a shutdown callback on the GUI thread.
            if (!bus.send(message)) throw std::runtime_error(bus.lastError().message().toStdString());
        }
    }
    ~SleepInhibit() { QDBusConnection::disconnectFromBus(name); }
    // Wire progress values are interpreted only by qt::sleep_inhibit.
    int poll() {
        if (!bus.isConnected() && state != State::Closed) {
            fail(bus.lastError());
            state = State::Unavailable;
        }
        if (state == State::Pending && pending && pending->isFinished()) {
            QDBusPendingReply<QDBusObjectPath> reply(*pending);
            if (reply.isError()) fail(reply.error());
            else {
                const auto returned = reply.value().path();
                if (!returned.startsWith(QStringLiteral("/org/freedesktop/portal/desktop/request/"))) {
                    failure = QStringLiteral("Inhibit returned an invalid request path");
                    state = State::Failed;
                } else {
                    if (returned != path) {
                        accepted = false;
                        bus.disconnect(service, path, QStringLiteral("org.freedesktop.portal.Request"),
                                       QStringLiteral("Response"), this, SLOT(response(uint,QVariantMap)));
                        path = returned;
                        if (!bus.connect(service, path, QStringLiteral("org.freedesktop.portal.Request"),
                                         QStringLiteral("Response"), this, SLOT(response(uint,QVariantMap))))
                            fail(bus.lastError());
                    }
                    replied = true;
                }
            }
            pending.reset();
        }
        if (state == State::Pending && replied && accepted) state = State::Held;
        if (state == State::Pending && elapsed.isValid() && elapsed.elapsed() >= CallTimeoutMs) {
            failure = QStringLiteral("portal did not confirm Inhibit with a successful Response before the deadline");
            state = State::Failed;
        }
        switch (state) {
        case State::Pending: return 0;
        case State::Held: return 1;
        case State::Unavailable: return 2;
        case State::Failed: return 3;
        case State::Closed: return 4;
        }
        return 3;
    }
    QString error() const { return failure; }
};
inline std::unique_ptr<SleepInhibit> acquireSleepInhibit(const QString &reason) {
    return std::make_unique<SleepInhibit>(reason);
}
#endif
#endif
