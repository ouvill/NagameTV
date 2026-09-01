#include "player_controller.h"

#include <QByteArray>
#include <QTimer>

#include <array>

PlayerController *PlayerController::instance_ = nullptr;

PlayerController::PlayerController(QObject *parent) : QObject(parent) {
    instance_ = this;
    const auto configuredServer = qEnvironmentVariable("MIRAKURUN_SERVER");
    const auto configuredServiceId = qEnvironmentVariable("MIRAKURUN_SERVICE_ID");
    if (!configuredServer.isEmpty())
        server_ = configuredServer;
    if (!configuredServiceId.isEmpty())
        serviceId_ = configuredServiceId;
    std::array<char, 512> error{};
    player_ = mirakurun_player_create(error.data(), error.size());
    if (!player_)
        setStatus(QString::fromUtf8(error.data()));

    auto *timer = new QTimer(this);
    timer->setInterval(50);
    connect(timer, &QTimer::timeout, this, &PlayerController::pollEvents);
    timer->start();
}

PlayerController::~PlayerController() {
    if (player_)
        mirakurun_player_destroy(player_);
    instance_ = nullptr;
}

PlayerController *PlayerController::instance() { return instance_; }

bool PlayerController::attachVideoItem(void *item) {
    if (!player_)
        return false;
    std::array<char, 512> error{};
    if (!mirakurun_player_attach_video_item(player_, item, error.data(),
                                             error.size())) {
        setStatus(QString::fromUtf8(error.data()));
        return false;
    }
    return true;
}

void PlayerController::setServer(const QString &value) {
    if (server_ == value)
        return;
    server_ = value.trimmed();
    emit serverChanged();
}

void PlayerController::setServiceId(const QString &value) {
    if (serviceId_ == value)
        return;
    serviceId_ = value.trimmed();
    emit serviceIdChanged();
}

void PlayerController::setVolume(double value) {
    value = qBound(0.0, value, 100.0);
    if (qFuzzyCompare(volume_, value))
        return;
    volume_ = value;
    if (player_)
        mirakurun_player_set_volume(player_, volume_);
    emit volumeChanged();
}

void PlayerController::play() {
    if (!player_) {
        setStatus(tr("Could not initialize the player"));
        return;
    }
    bool ok = false;
    const auto id = serviceId_.toULongLong(&ok);
    if (!ok || id == 0) {
        setStatus(tr("Enter a Mirakurun service ID"));
        return;
    }

    const QByteArray server = server_.toUtf8();
    std::array<char, 512> error{};
    if (!mirakurun_player_play_service(player_, server.constData(), id,
                                        error.data(), error.size())) {
        setStatus(QString::fromUtf8(error.data()));
        return;
    }
    paused_ = false;
    setPlaying(true);
    setStatus(tr("Connecting..."));
}

void PlayerController::stop() {
    if (player_)
        mirakurun_player_stop(player_);
    paused_ = false;
    setPlaying(false);
    setStatus(tr("Stopped"));
}

void PlayerController::togglePause() {
    if (!player_ || !playing_)
        return;
    paused_ = !paused_;
    mirakurun_player_set_pause(player_, paused_);
    setStatus(paused_ ? tr("Paused") : tr("Playing"));
}

void PlayerController::setStatus(QString value) {
    if (status_ == value)
        return;
    status_ = std::move(value);
    emit statusChanged();
}

void PlayerController::setPlaying(bool value) {
    if (playing_ == value)
        return;
    playing_ = value;
    emit playingChanged();
}

void PlayerController::pollEvents() {
    if (!player_)
        return;
    const int event = mirakurun_player_drain_events(player_);
    if (event == 1)
        setStatus(tr("Playing"));
    else if (event == 2) {
        setPlaying(false);
        setStatus(tr("Stream ended"));
    } else if (event == 3) {
        setPlaying(false);
        setStatus(tr("Playback error"));
    }
}
