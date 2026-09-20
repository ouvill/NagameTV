#ifndef NAGAMETV_DESKTOP_MEDIA_H
#define NAGAMETV_DESKTOP_MEDIA_H
#include <QtCore/QtGlobal>
// Linux MPRIS protocol boundary. Playback policy and command execution stay in Rust.
#ifdef Q_OS_LINUX
#include <QtCore/QCoreApplication>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QQueue>
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusError>
#include <QtDBus/QDBusMessage>
#include <QtDBus/QDBusObjectPath>
#include <QtDBus/QDBusVariant>
#include <QtDBus/QDBusVirtualObject>
#include <cmath>
#include <memory>
#include <stdexcept>

class DesktopMedia final : public QDBusVirtualObject {
    static constexpr int MaxPendingCommands = 32;
    static constexpr double RateStepsPerUnit = 10.0;
    static constexpr double RateStepTolerance = 0.000001;
    const QString path = QStringLiteral("/org/mpris/MediaPlayer2");
    const QString playerInterface = QStringLiteral("org.mpris.MediaPlayer2.Player");
    const QString rootInterface = QStringLiteral("org.mpris.MediaPlayer2");
    const QString propertiesInterface = QStringLiteral("org.freedesktop.DBus.Properties");
    QDBusConnection bus;
    QString service;
    QVariantMap playerProperties;
    QJsonObject snapshot;
    QQueue<QString> commands;
    bool wasSeeking = false;
    explicit DesktopMedia(QDBusConnection connection) : bus(connection) {}

    QVariantMap rootProperties() const {
        return {{"CanQuit", false}, {"CanRaise", true}, {"HasTrackList", false},
                {"Identity", "NagameTV"}, {"DesktopEntry", "io.github.ouvill.nagametv"},
                {"SupportedUriSchemes", QStringList()}, {"SupportedMimeTypes", QStringList()}};
    }
    bool error(const QDBusMessage &message, const QDBusConnection &connection,
               QDBusError::ErrorType type, const QString &text) const {
        connection.send(message.createErrorReply(type, text));
        return true;
    }
    bool enqueue(const QDBusMessage &message, const QDBusConnection &connection,
                 const QString &kind, double value = 0, const QString &track = {}) {
        if (commands.size() >= MaxPendingCommands)
            return error(message, connection, QDBusError::LimitsExceeded, "Too many pending media commands");
        commands.enqueue(QString::fromUtf8(QJsonDocument(QJsonObject{
            {"kind", kind}, {"value", value}, {"track", track}}).toJson(QJsonDocument::Compact)));
        connection.send(message.createReply());
        return true;
    }
public:
    // Only the factory can construct an instance; success proves registration.
    static std::unique_ptr<DesktopMedia> connect() {
        auto bus = QDBusConnection::sessionBus();
        if (!bus.isConnected()) throw std::runtime_error("MPRIS session bus is unavailable");
        auto object = std::unique_ptr<DesktopMedia>(new DesktopMedia(bus));
        if (!bus.registerVirtualObject(object->path, object.get(), QDBusConnection::SingleNode))
            throw std::runtime_error(bus.lastError().message().toStdString());
        object->service = QStringLiteral("org.mpris.MediaPlayer2.io.github.ouvill.nagametv.instance%1")
                              .arg(QCoreApplication::applicationPid());
        if (!bus.registerService(object->service)) {
            bus.unregisterObject(object->path);
            object->service.clear();
            throw std::runtime_error(bus.lastError().message().toStdString());
        }
        return object;
    }
    ~DesktopMedia() override {
        if (!service.isEmpty()) {
            bus.unregisterService(service);
            bus.unregisterObject(path);
        }
    }
    QString takeCommand() { return commands.isEmpty() ? QString() : commands.dequeue(); }
    void publish(const QString &json) {
        auto next = QJsonDocument::fromJson(json.toUtf8()).object();
        QVariantMap metadata;
        if (!next.value("track").toString().isEmpty()) {
            metadata.insert("mpris:trackid", QVariant::fromValue(QDBusObjectPath(next.value("track").toString())));
            metadata.insert("xesam:title", next.value("title").toString());
            if (!next.value("artist").toString().isEmpty())
                metadata.insert("xesam:artist", QStringList{next.value("artist").toString()});
            if (next.value("length").toDouble() > 0)
                metadata.insert("mpris:length", next.value("length").toVariant().toLongLong());
        }
        QVariantMap properties{{"PlaybackStatus", next.value("status").toString()},
            {"Rate", next.value("rate").toDouble(1.0)}, {"Metadata", metadata}, {"Volume", next.value("volume").toDouble()},
            {"Position", next.value("position").toVariant().toLongLong()},
            {"MinimumRate", next.value("minimum_rate").toDouble(1.0)}, {"MaximumRate", next.value("maximum_rate").toDouble(1.0)}, {"CanGoNext", false}, {"CanGoPrevious", false},
            {"CanPlay", next.value("can_play").toBool()}, {"CanPause", next.value("can_pause").toBool()},
            {"CanSeek", next.value("can_seek").toBool()}, {"CanControl", true}};
        QVariantMap changed;
        for (auto it = properties.cbegin(); it != properties.cend(); ++it)
            if (it.key() != "Position" && playerProperties.value(it.key()) != it.value())
                changed.insert(it.key(), it.value());
        const bool seekCompleted = wasSeeking && !next.value("seeking").toBool()
            && snapshot.value("track") == next.value("track");
        playerProperties = properties;
        snapshot = next;
        wasSeeking = next.value("seeking").toBool();
        if (!changed.isEmpty()) {
            auto signal = QDBusMessage::createSignal(path, propertiesInterface, "PropertiesChanged");
            signal << playerInterface << changed << QStringList();
            bus.send(signal);
        }
        if (seekCompleted) {
            auto signal = QDBusMessage::createSignal(path, playerInterface, "Seeked");
            signal << playerProperties.value("Position");
            bus.send(signal);
        }
    }
    QString introspect(const QString &) const override {
        return QStringLiteral(R"XML(
<interface name="org.freedesktop.DBus.Introspectable">
 <method name="Introspect"><arg type="s" direction="out"/></method>
</interface>
<interface name="org.mpris.MediaPlayer2">
 <method name="Raise"/><method name="Quit"/>
 <property name="CanQuit" type="b" access="read"/><property name="CanRaise" type="b" access="read"/>
 <property name="HasTrackList" type="b" access="read"/><property name="Identity" type="s" access="read"/>
 <property name="DesktopEntry" type="s" access="read"/>
 <property name="SupportedUriSchemes" type="as" access="read"/><property name="SupportedMimeTypes" type="as" access="read"/>
</interface>
<interface name="org.mpris.MediaPlayer2.Player">
 <method name="Play"/><method name="Pause"/><method name="PlayPause"/><method name="Stop"/>
 <method name="Next"/><method name="Previous"/>
 <method name="Seek"><arg name="Offset" type="x" direction="in"/></method>
 <method name="SetPosition"><arg name="TrackId" type="o" direction="in"/><arg name="Position" type="x" direction="in"/></method>
 <property name="PlaybackStatus" type="s" access="read"/><property name="Rate" type="d" access="readwrite"/>
 <property name="Metadata" type="a{sv}" access="read"/><property name="Volume" type="d" access="readwrite"/>
 <property name="Position" type="x" access="read"><annotation name="org.freedesktop.DBus.Property.EmitsChangedSignal" value="false"/></property>
 <property name="MinimumRate" type="d" access="read"/><property name="MaximumRate" type="d" access="read"/>
 <property name="CanGoNext" type="b" access="read"/><property name="CanGoPrevious" type="b" access="read"/>
 <property name="CanPlay" type="b" access="read"/><property name="CanPause" type="b" access="read"/>
 <property name="CanSeek" type="b" access="read"/><property name="CanControl" type="b" access="read"/>
 <signal name="Seeked"><arg name="Position" type="x"/></signal>
</interface>
<interface name="org.freedesktop.DBus.Properties">
 <method name="Get"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
 <method name="GetAll"><arg type="s" direction="in"/><arg type="a{sv}" direction="out"/></method>
 <method name="Set"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/></method>
 <signal name="PropertiesChanged"><arg type="s"/><arg type="a{sv}"/><arg type="as"/></signal>
</interface>)XML");
    }
    bool handleMessage(const QDBusMessage &message, const QDBusConnection &connection) override {
        const auto args = message.arguments();
        const auto method = message.member();
        const auto signature = message.signature();
        if (message.interface() == "org.freedesktop.DBus.Introspectable" && method == "Introspect" && signature.isEmpty()) {
            const auto xml = QStringLiteral("<node>") + introspect(path) + QStringLiteral("</node>");
            connection.send(message.createReply(QVariantList{xml})); return true;
        }
        if (message.interface() == "org.freedesktop.DBus.Peer" && method == "Ping" && signature.isEmpty()) {
            connection.send(message.createReply()); return true;
        }
        if (message.interface() == propertiesInterface) {
            if ((method == "Get" && signature == "ss") || (method == "GetAll" && signature == "s")
                || (method == "Set" && signature == "ssv")) {
                const auto interface = args[0].toString();
                if (interface != rootInterface && interface != playerInterface)
                    return error(message, connection, QDBusError::UnknownInterface, "Unknown media interface");
                const auto properties = interface == rootInterface ? rootProperties() : playerProperties;
                if (method == "GetAll") {
                    connection.send(message.createReply(QVariantList{properties})); return true;
                }
                const auto name = args[1].toString();
                if (!properties.contains(name))
                    return error(message, connection, QDBusError::UnknownProperty, "Unknown media property");
                if (method == "Get") {
                    connection.send(message.createReply(QVariantList{QVariant::fromValue(QDBusVariant(properties.value(name)))})); return true;
                }
                if (interface != playerInterface || (name != "Volume" && name != "Rate"))
                    return error(message, connection, QDBusError::PropertyReadOnly, "Media property is read only");
                const auto value = qvariant_cast<QDBusVariant>(args[2]).variant();
                if (value.metaType().id() != QMetaType::Double || !std::isfinite(value.toDouble()))
                    return error(message, connection, QDBusError::InvalidArgs, "Expected a finite double");
                if (name == "Volume") return enqueue(message, connection, "Volume", value.toDouble());
                if (value.toDouble() == 0) return enqueue(message, connection, "Pause");
                const double rate = value.toDouble();
                if (rate < playerProperties.value("MinimumRate").toDouble()
                    || rate > playerProperties.value("MaximumRate").toDouble()
                    || std::abs(rate * RateStepsPerUnit - std::round(rate * RateStepsPerUnit)) > RateStepTolerance)
                    return error(message, connection, QDBusError::InvalidArgs, "Unsupported playback rate");
                return enqueue(message, connection, "Rate", rate, snapshot.value("track").toString());
            }
            return error(message, connection, QDBusError::InvalidArgs, "Invalid property arguments");
        }
        if (message.interface() == rootInterface && signature.isEmpty()) {
            if (method == "Raise") return enqueue(message, connection, "Raise");
            if (method == "Quit") { connection.send(message.createReply()); return true; }
        }
        if (message.interface() == playerInterface) {
            if (signature.isEmpty()) {
                if (method == "Play" || method == "Pause" || method == "Stop")
                    return enqueue(message, connection, method);
                if (method == "PlayPause") {
                    if (!playerProperties.value("CanPause").toBool())
                        return error(message, connection, QDBusError::NotSupported, "This live stream cannot pause");
                    return enqueue(message, connection, method);
                }
                if (method == "Next" || method == "Previous") { connection.send(message.createReply()); return true; }
            }
            if ((method == "Seek" && signature == "x") || (method == "SetPosition" && signature == "ox")) {
                const auto track = snapshot.value("track").toString();
                const auto position = args.last().toLongLong();
                const bool validTrack = method == "Seek" || qvariant_cast<QDBusObjectPath>(args[0]).path() == track;
                const bool validPosition = method == "Seek" || (position >= 0 && position <= snapshot.value("length").toDouble());
                if (playerProperties.value("CanSeek").toBool() && validTrack && validPosition)
                    return enqueue(message, connection, method, double(position), track);
                connection.send(message.createReply()); return true;
            }
        }
        return error(message, connection, QDBusError::UnknownMethod, "Unknown media method or signature");
    }
};
inline std::unique_ptr<DesktopMedia> connectDesktopMedia() { return DesktopMedia::connect(); }
#endif
#endif
