#include "player_controller.h"

#include <QByteArray>
#include <QTimer>

#include <array>

PlayerController *PlayerController::instance_ = nullptr;

PlayerController::PlayerController(QObject *parent) : QObject(parent) {
    instance_ = this;
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
        setStatus(QStringLiteral("プレイヤーを初期化できませんでした"));
        return;
    }
    bool ok = false;
    const auto id = serviceId_.toULongLong(&ok);
    if (!ok || id == 0) {
        setStatus(QStringLiteral("Mirakurun の service ID を入力してください"));
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
    setStatus(QStringLiteral("接続中…"));
}

void PlayerController::stop() {
    if (player_)
        mirakurun_player_stop(player_);
    paused_ = false;
    setPlaying(false);
    setStatus(QStringLiteral("停止しました"));
}

void PlayerController::togglePause() {
    if (!player_ || !playing_)
        return;
    paused_ = !paused_;
    mirakurun_player_set_pause(player_, paused_);
    setStatus(paused_ ? QStringLiteral("一時停止") : QStringLiteral("再生中"));
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
        setStatus(QStringLiteral("再生中"));
    else if (event == 2) {
        setPlaying(false);
        setStatus(QStringLiteral("ストリームが終了しました"));
    } else if (event == 3) {
        setPlaying(false);
        setStatus(QStringLiteral("再生エラー"));
    }
}
