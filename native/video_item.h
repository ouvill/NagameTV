#pragma once

#include <QQuickFramebufferObject>

class VideoItem : public QQuickFramebufferObject {
    Q_OBJECT
    QML_ELEMENT

public:
    explicit VideoItem(QQuickItem *parent = nullptr);
    Renderer *createRenderer() const override;
};
