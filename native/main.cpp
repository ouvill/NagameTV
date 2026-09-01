#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTimer>

#include "player_controller.h"

int main(int argc, char *argv[]) {
    QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);
    QGuiApplication app(argc, argv);
    QCoreApplication::setOrganizationName(QStringLiteral("MirakurunViewer"));
    QCoreApplication::setApplicationName(QStringLiteral("Mirakurun Viewer"));

    PlayerController player;
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty(QStringLiteral("player"), &player);
    engine.load(QUrl(QStringLiteral("qrc:/MirakurunViewer/qml/Main.qml")));
    if (engine.rootObjects().isEmpty())
        return 1;
    auto *videoItem = engine.rootObjects().constFirst()->findChild<QQuickItem *>(
        QStringLiteral("videoItem"));
    if (!videoItem || !player.attachVideoItem(videoItem))
        return 1;
    if (qEnvironmentVariableIntValue("MIRAKURUN_AUTOPLAY") != 0)
        QTimer::singleShot(0, &player, &PlayerController::play);
    return app.exec();
}
