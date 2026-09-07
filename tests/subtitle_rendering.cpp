#include "subtitle_outline.h"
#include <QtQuickTest/quicktest.h>
#include <QtQml/QQmlContext>
#include <QtQml/QQmlEngine>

// Exercise the production geometry helper through the same QML method contract.
class OutlineProvider final : public QObject {
    Q_OBJECT
    Q_PROPERTY(int calls READ calls NOTIFY callsChanged)
public:
    using QObject::QObject;
    int calls() const { return calls_; }
    Q_INVOKABLE QString subtitle_glyph_outline(const QString &text, const QFont &font) {
        ++calls_;
        emit callsChanged();
        return subtitleOutlinePath(text, font);
    }
signals:
    void callsChanged();
private:
    int calls_ = 0;
};

class Setup final : public QObject {
    Q_OBJECT
public slots:
    void qmlEngineAvailable(QQmlEngine *engine) {
        // Used only by the fixed-corpus memory benchmark, not by production QML.
        engine->rootContext()->setContextProperty(
            QStringLiteral("subtitleBenchmarkStroke"),
            qEnvironmentVariable("VIEWER_SUBTITLE_BENCHMARK_STROKE") != QStringLiteral("0"));
        engine->rootContext()->setContextProperty(
            QStringLiteral("subtitleOutlines"), new OutlineProvider(engine));
    }
};

QUICK_TEST_MAIN_WITH_SETUP(subtitle_rendering, Setup)
#include "subtitle_rendering.moc"
