#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>

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
    return app.exec();
}
