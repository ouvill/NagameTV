#ifndef VIEWER_MEDIA_CAPTION_H
#define VIEWER_MEDIA_CAPTION_H
#include <QtQuick/QQuickPaintedItem>
#include <QtGui/QPainter>
#include <QtGui/QImage>
inline void paintMediaCaption(QPainter *painter, const QImage &image, double width, double height) {
    if (!painter || image.isNull()) return;
    QSizeF size = image.size();
    size.scale(QSizeF(width, height), Qt::KeepAspectRatio);
    painter->drawImage(QRectF((width - size.width()) / 2, (height - size.height()) / 2, size.width(), size.height()), image);
}
#endif
