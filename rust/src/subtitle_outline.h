#ifndef MIRAKURUN_VIEWER_SUBTITLE_OUTLINE_H
#define MIRAKURUN_VIEWER_SUBTITLE_OUTLINE_H

#include <QtCore/QString>
#include <QtGui/QFont>
#include <QtGui/QPainterPath>

// Preserve the font's baseline-relative coordinates, including negative y.
// PathText normalizes each glyph to its ink's top edge, losing this placement.
inline QString subtitleOutlinePathData(const QPainterPath &path) {
  QString svg;
  auto point = [](const QPainterPath::Element &element) {
    return QString::number(element.x, 'g', 17) + QLatin1Char(' ')
        + QString::number(element.y, 'g', 17) + QLatin1Char(' ');
  };
  for (int i = 0; i < path.elementCount(); ++i) {
    const auto element = path.elementAt(i);
    if (element.isMoveTo()) {
      if (!svg.isEmpty()) svg += QStringLiteral("Z ");
      svg += QStringLiteral("M ") + point(element);
    } else if (element.isLineTo()) {
      svg += QStringLiteral("L ") + point(element);
    } else if (element.isCurveTo()) {
      // QPainterPath stores each cubic as CurveTo followed by exactly two
      // CurveToData elements. addText builds valid paths under this Qt contract.
      svg += QStringLiteral("C ") + point(element)
          + point(path.elementAt(i + 1)) + point(path.elementAt(i + 2));
      i += 2;
    }
  }
  if (!svg.isEmpty()) svg += QLatin1Char('Z');
  return svg;
}

inline QString subtitleOutlinePath(const QString &text, const QFont &font) {
  QPainterPath path;
  path.addText(0, 0, font, text);
  return subtitleOutlinePathData(path);
}

#endif // MIRAKURUN_VIEWER_SUBTITLE_OUTLINE_H
