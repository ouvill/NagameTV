use super::bridge::ffi;
use cxx_qt_lib::{
    QByteArray, QColor, QCoreApplication, QImage, QImageFormat, QPainterPath, QPoint, QPointF,
    QRectF,
};

fn check_placement(path: &QPainterPath, name: &str) {
    let mut expected =
        QImage::from_width_height_and_format(120, 100, QImageFormat::Format_ARGB32_Premultiplied);
    expected.fill(&QColor::from_rgba_f(0.0, 0.0, 0.0, 0.0));
    ffi::raster_path(path, &mut expected, &QPoint::new(30, 70));
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='120' height='100' viewBox='0 0 120 100'><path transform='translate(30 70)' fill='white' fill-rule='nonzero' d='{}'/></svg>",
        ffi::outline_data(path)
    );
    let mut renderer = ffi::new_svg(&QByteArray::from(svg.as_str()));
    assert!(renderer.is_valid(), "Invalid SVG: {name}");
    let mut actual =
        QImage::from_width_height_and_format(120, 100, QImageFormat::Format_ARGB32_Premultiplied);
    actual.fill(&QColor::from_rgba_f(0.0, 0.0, 0.0, 0.0));
    ffi::raster_svg(renderer.pin_mut(), &mut actual);
    assert!(
        actual == expected,
        "Baseline-relative geometry changed: {name}"
    );
}

pub fn run() -> i32 {
    // In-memory raster geometry only: no display, GPU, fonts or audio devices.
    let app = QCoreApplication::new();
    assert!(!app.is_null());
    let mut full = QPainterPath::default();
    full.add_rect(&QRectF::new(2.0, -38.0, 32.0, 36.0));
    check_placement(&full, "full height");
    let mut small = QPainterPath::default();
    small.add_ellipse(&QRectF::new(8.0, -18.0, 18.0, 15.0));
    check_placement(&small, "small ink bounds and cubic curves");
    let mut bar = QPainterPath::default();
    bar.add_rect(&QRectF::new(1.0, -21.0, 34.0, 3.0));
    check_placement(&bar, "midline bar");
    let mut overhang = QPainterPath::from(QPointF::new(-5.0, -36.0));
    overhang.cubic_to(
        &QPointF::new(22.0, -40.0),
        &QPointF::new(39.0, -12.0),
        &QPointF::new(28.0, 4.0),
    );
    overhang.line_to(&QPointF::new(-5.0, -36.0));
    overhang.close_subpath();
    check_placement(&overhang, "negative bearing and descender");
    let mut combined = full.clone();
    combined.add_path(&bar.translated(&QPointF::new(40.0, 0.0)));
    combined.set_fill_rule(cxx_qt_lib::FillRule::WindingFill);
    check_placement(&combined, "separate contours on the same baseline");
    check_placement(&QPainterPath::default(), "empty glyph");
    println!("Subtitle outline placement checks passed (6 raster comparisons)");
    0
}
