#include "subtitle_outline.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QDebug>
#include <QtGui/QImage>
#include <QtGui/QPainter>
#include <QtSvg/QSvgRenderer>
#include <cstdlib>

// Test geometry in memory: no display, GPU, font service or audio device.
// Ink bounds deliberately differ while all paths share the same baseline.
static void checkPlacement(const QPainterPath &path, const char *name) {
  QImage expected(120, 100, QImage::Format_ARGB32_Premultiplied);
  expected.fill(Qt::transparent);
  {
    QPainter painter(&expected);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.translate(30, 70);
    painter.setPen(Qt::NoPen);
    painter.setBrush(Qt::white);
    painter.drawPath(path);
  }

  const auto svg = QStringLiteral(
      "<svg xmlns='http://www.w3.org/2000/svg' width='120' height='100' viewBox='0 0 120 100'>"
      "<path transform='translate(30 70)' fill='white' fill-rule='nonzero' d='%1'/></svg>")
      .arg(subtitleOutlinePathData(path)).toUtf8();
  QSvgRenderer renderer(svg);
  if (!renderer.isValid()) qFatal("Invalid SVG: %s", name);
  QImage actual(expected.size(), expected.format());
  actual.fill(Qt::transparent);
  {
    QPainter painter(&actual);
    renderer.render(&painter);
  }
  if (actual != expected) qFatal("Baseline-relative geometry changed: %s", name);
}

int main(int argc, char **argv) {
  QCoreApplication app(argc, argv);
  QPainterPath fullHeight;
  fullHeight.addRect(2, -38, 32, 36);
  checkPlacement(fullHeight, "full height");
  QPainterPath small;
  small.addEllipse(QRectF(8, -18, 18, 15));
  checkPlacement(small, "small ink bounds and cubic curves");
  QPainterPath bar;
  bar.addRect(1, -21, 34, 3);
  checkPlacement(bar, "midline bar");
  QPainterPath overhang;
  overhang.moveTo(-5, -36);
  overhang.cubicTo(22, -40, 39, -12, 28, 4);
  overhang.lineTo(-5, -36);
  overhang.closeSubpath();
  checkPlacement(overhang, "negative bearing and descender");
  QPainterPath combined = fullHeight;
  combined.addPath(bar.translated(40, 0));
  combined.setFillRule(Qt::WindingFill);
  checkPlacement(combined, "separate contours on the same baseline");
  checkPlacement(QPainterPath(), "empty glyph");
  qInfo("Subtitle outline placement tests passed");
}
