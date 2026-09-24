#ifndef VIEWER_SCREENSHOT_PAINTER_H
#define VIEWER_SCREENSHOT_PAINTER_H
#include "cxx-qt-lib/qpainterpath.h"
#include "cxx-qt-lib/qfont.h"
#include "cxx-qt-lib/qimage.h"
#include "cxx-qt-lib/qcolor.h"
#include "cxx-qt-lib/qstring.h"
#include <QtGui/QImage>
#include <QtGui/QPainter>
#include <QtGui/QPainterPath>
#include <QtGui/QFontMetricsF>
#include <QtGui/QTextLayout>
#include <QtGui/QGlyphRun>
#include <algorithm>
#include <cmath>
#include <vector>
namespace viewer_screenshot {
// Bound QPainter's retained QPaintDevice* to this image's exclusive borrow.
template<typename Overlay>
inline void paintImage(QImage &image, const Overlay &overlay) {
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setRenderHint(QPainter::TextAntialiasing);
    overlay.paint(painter, image.width(), image.height());
}
inline void transform(QPainter &painter, double x, double y, double sx, double sy) {
    painter.translate(x, y); painter.scale(sx, sy);
}
inline void rotate(QPainter &painter, double x, double y, double degrees) {
    painter.translate(x, y); painter.rotate(degrees); painter.translate(-x, -y);
}
inline void clip(QPainter &painter, double x, double y, double w, double h) {
    painter.setClipRect(QRectF(x, y, w, h), Qt::IntersectClip);
}
inline void rectangle(QPainter &painter, double x, double y, double w, double h, const QColor &color) {
    painter.fillRect(QRectF(x, y, w, h), color);
}
inline QPainterPath textPath(const QString &text, const QFont &font, double width, bool wrap, bool center) {
    QPainterPath result;
    result.setFillRule(Qt::WindingFill);
    if (!wrap) { result.addText(0, 0, font, text); return result; }
    // Qt Quick's plain caption uses the same text layout engine. Keep the
    // recorded logical width so saving at another resolution never reflows it.
    auto paragraphs = text;
    paragraphs.replace('\n', QChar::LineSeparator);
    QTextLayout layout(paragraphs, font);
    QTextOption option;
    option.setWrapMode(QTextOption::WrapAtWordBoundaryOrAnywhere);
    option.setAlignment(center ? Qt::AlignHCenter : Qt::AlignLeft);
    layout.setTextOption(option);
    layout.beginLayout();
    double y = 0;
    double firstAscent = 0;
    while (true) {
        auto line = layout.createLine();
        if (!line.isValid()) break;
        line.setLineWidth(width);
        if (layout.lineCount() == 1) firstAscent = line.ascent();
        line.setPosition(QPointF(0, y - firstAscent));
        y += line.height();
    }
    layout.endLayout();
    for (const auto &run : layout.glyphRuns()) {
        const auto positions = run.positions();
        const auto glyphs = run.glyphIndexes();
        for (qsizetype i = 0; i < glyphs.size(); ++i)
            result.addPath(run.rawFont().pathForGlyph(glyphs[i]).translated(positions[i]));
    }
    if (font.underline()) {
        const QFontMetricsF metrics(font);
        for (int index = 0; index < layout.lineCount(); ++index) {
            const auto line = layout.lineAt(index);
            const auto bounds = line.naturalTextRect();
            result.addRect(bounds.x(), line.y() + line.ascent() + metrics.underlinePos(),
                           bounds.width(), metrics.lineWidth());
        }
    }
    return result;
}
inline void path(QPainter &painter, const QPainterPath &shape, const QColor &color,
                 const QColor &stroke, double strokeWidth) {
    painter.setBrush(Qt::NoBrush);
    QPen pen(stroke, strokeWidth, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin);
    painter.setPen(strokeWidth > 0 ? pen : QPen(Qt::NoPen));
    if (strokeWidth > 0) painter.drawPath(shape);
    painter.fillPath(shape, color);
}
inline void shadow(QPainter &painter, const QPainterPath &shape, double offset, double radius) {
    // Rasterize only the portion visible in the destination image. Blur in
    // destination pixels so its size follows the captured logical transform.
    const auto transform = painter.worldTransform();
    auto shifted = shape.translated(offset, offset);
    auto devicePath = transform.map(shifted);
    const int spread = std::max(1, int(std::ceil(radius * std::hypot(transform.m11(), transform.m12()))));
    const QRect visible(0, 0, painter.device()->width(), painter.device()->height());
    const QRect bounds = devicePath.boundingRect().adjusted(-2*spread, -2*spread, 2*spread, 2*spread)
        .toAlignedRect().intersected(visible.adjusted(-spread, -spread, spread, spread));
    if (bounds.isEmpty()) return;
    QImage mask(bounds.size(), QImage::Format_ARGB32_Premultiplied);
    mask.fill(Qt::transparent);
    {
        QPainter raster(&mask);
        raster.setRenderHint(QPainter::Antialiasing);
        raster.translate(-bounds.topLeft());
        raster.fillPath(devicePath, QColor(0, 0, 0, 128));
    }
    // Two separable box passes approximate the soft Qt Quick comment shadow.
    const int w = mask.width(), h = mask.height();
    std::vector<unsigned char> alpha(size_t(w)*h), scratch(alpha.size());
    for (int y=0; y<h; ++y) {
        auto *row = reinterpret_cast<const QRgb *>(mask.constScanLine(y));
        for (int x=0; x<w; ++x) alpha[size_t(y)*w+x] = qAlpha(row[x]);
    }
    for (int pass=0; pass<2; ++pass) {
        for (int y=0; y<h; ++y) {
            int sum = 0;
            for (int x=-spread; x<w; ++x) {
                if (x+spread < w) sum += alpha[size_t(y)*w+x+spread];
                if (x-spread-1 >= 0) sum -= alpha[size_t(y)*w+x-spread-1];
                if (x >= 0) scratch[size_t(y)*w+x] = sum/(2*spread+1);
            }
        }
        for (int x=0; x<w; ++x) {
            int sum = 0;
            for (int y=-spread; y<h; ++y) {
                if (y+spread < h) sum += scratch[size_t(y+spread)*w+x];
                if (y-spread-1 >= 0) sum -= scratch[size_t(y-spread-1)*w+x];
                if (y >= 0) alpha[size_t(y)*w+x] = sum/(2*spread+1);
            }
        }
    }
    for (int y=0; y<h; ++y) {
        auto *row = reinterpret_cast<QRgb *>(mask.scanLine(y));
        for (int x=0; x<w; ++x) row[x] = qRgba(0, 0, 0, alpha[size_t(y)*w+x]);
    }
    painter.save();
    painter.resetTransform();
    painter.drawImage(bounds.topLeft(), mask);
    painter.restore();
}
}
namespace viewer_screenshot {
inline void mediaCaption(QPainter &painter, const QImage &image, int width, int height) { painter.drawImage(QRectF(0, 0, width, height), image); }
}
#endif
