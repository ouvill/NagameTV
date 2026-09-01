#pragma once

#include <QObject>
#include <QString>

#include "rust_core.h"

class PlayerController final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString server READ server WRITE setServer NOTIFY serverChanged)
    Q_PROPERTY(QString serviceId READ serviceId WRITE setServiceId NOTIFY serviceIdChanged)
    Q_PROPERTY(QString status READ status NOTIFY statusChanged)
    Q_PROPERTY(bool playing READ playing NOTIFY playingChanged)
    Q_PROPERTY(double volume READ volume WRITE setVolume NOTIFY volumeChanged)

public:
    explicit PlayerController(QObject *parent = nullptr);
    ~PlayerController() override;

    static PlayerController *instance();
    MirakurunPlayer *nativePlayer() const { return player_; }
    bool attachVideoItem(void *item);

    QString server() const { return server_; }
    QString serviceId() const { return serviceId_; }
    QString status() const { return status_; }
    bool playing() const { return playing_; }
    double volume() const { return volume_; }

    void setServer(const QString &value);
    void setServiceId(const QString &value);
    void setVolume(double value);

    Q_INVOKABLE void play();
    Q_INVOKABLE void stop();
    Q_INVOKABLE void togglePause();

signals:
    void serverChanged();
    void serviceIdChanged();
    void statusChanged();
    void playingChanged();
    void volumeChanged();

private:
    void setStatus(QString value);
    void setPlaying(bool value);
    void pollEvents();

    static PlayerController *instance_;
    MirakurunPlayer *player_ = nullptr;
    QString server_ = QStringLiteral("http://127.0.0.1:40772");
    QString serviceId_;
    QString status_ = QStringLiteral("Ready");
    bool playing_ = false;
    bool paused_ = false;
    double volume_ = 70.0;
};
