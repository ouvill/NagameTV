//! Immutable, display-coordinate drawing commands. Layout is captured on the
//! GUI thread; glyph outlines and image composition run on a saving worker.
use super::Error;
use cxx_qt_lib::{QColor, QFont, QImage, QPainter, QString};
use serde::Deserialize;
use std::pin::Pin;

#[cxx::bridge(namespace = "viewer_screenshot")]
mod ffi {
    unsafe extern "C++" {
        include!("screenshot_painter.h");
        #[namespace = ""]
        type QImage = cxx_qt_lib::QImage;
        #[namespace = ""]
        type QPainter = cxx_qt_lib::QPainter;
        #[namespace = ""]
        type QPainterPath = cxx_qt_lib::QPainterPath;
        #[namespace = ""]
        type QFont = cxx_qt_lib::QFont;
        #[namespace = ""]
        type QColor = cxx_qt_lib::QColor;
        #[namespace = ""]
        type QString = cxx_qt_lib::QString;
        fn paintImage(image: &mut QImage, overlay: &Overlay);
        fn mediaCaption(painter: Pin<&mut QPainter>, image: &QImage, width: i32, height: i32);
        fn transform(painter: Pin<&mut QPainter>, x: f64, y: f64, sx: f64, sy: f64);
        fn rotate(painter: Pin<&mut QPainter>, x: f64, y: f64, degrees: f64);
        fn clip(painter: Pin<&mut QPainter>, x: f64, y: f64, width: f64, height: f64);
        fn rectangle(
            painter: Pin<&mut QPainter>,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            color: &QColor,
        );
        fn textPath(
            text: &QString,
            font: &QFont,
            width: f64,
            wrap: bool,
            center: bool,
        ) -> QPainterPath;
        fn path(
            painter: Pin<&mut QPainter>,
            path: &QPainterPath,
            color: &QColor,
            stroke: &QColor,
            stroke_width: f64,
        );
        fn shadow(painter: Pin<&mut QPainter>, path: &QPainterPath, offset: f64, radius: f64);
    }
    extern "Rust" {
        type Overlay;
        fn paint(self: &Overlay, painter: Pin<&mut QPainter>, width: i32, height: i32);
    }
}

#[derive(Debug, Deserialize)]
pub struct Overlay {
    width: f64,
    height: f64,
    layers: Vec<Layer>,
    #[serde(skip)]
    media_caption: Option<QImage>,
}
#[derive(Debug, Deserialize)]
struct Layer {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    commands: Vec<Command>,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Command {
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        color: String,
        pose: Option<Pose>,
    },
    Text(Text),
}
#[derive(Debug, Deserialize)]
struct Text {
    pose: Option<Pose>,
    x: f64,
    y: f64,
    width: f64,
    baseline: f64,
    scale_x: f64,
    text: String,
    family: String,
    size: i32,
    bold: bool,
    italic: bool,
    underline: bool,
    color: String,
    stroke: String,
    stroke_width: f64,
    opacity: f64,
    shadow: Option<Shadow>,
    wrap: bool,
    center: bool,
}
#[derive(Debug, Deserialize)]
struct Pose {
    x: f64,
    y: f64,
    degrees: f64,
}
impl Pose {
    fn apply(&self, painter: Pin<&mut QPainter>) {
        ffi::rotate(painter, self.x, self.y, self.degrees);
    }
}
#[derive(Debug, Deserialize)]
struct Shadow {
    offset: f64,
    radius: f64,
}
const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;
const MAX_COMMANDS: usize = 2048;
const OVERLAY_LAYERS: usize = 2;
impl Overlay {
    pub fn media_caption_bytes(&self) -> usize {
        self.media_caption.as_ref().map_or(0, |image| {
            image.width().max(0) as usize * image.height().max(0) as usize * 4
        })
    }
    pub fn with_media_caption(mut self, image: &QImage) -> Self {
        self.media_caption = Some(crate::qt::ffi::share_screenshot_image(image));
        self
    }
    pub fn parse(json: &str) -> Result<Self, Error> {
        if json.len() > MAX_SNAPSHOT_BYTES {
            return Err(Error::Capacity);
        }
        let value: Self = serde_json::from_str(json).map_err(|_| Error::Encode)?;
        if value.width < 1.0
            || value.height < 1.0
            || value.layers.len() > OVERLAY_LAYERS
            || value
                .layers
                .iter()
                .map(|layer| layer.commands.len())
                .sum::<usize>()
                > MAX_COMMANDS
        {
            return Err(Error::Encode);
        }
        Ok(value)
    }
    #[cfg(test)]
    pub fn empty(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            layers: Vec::new(),
            media_caption: None,
        }
    }
    fn paint(&self, mut painter: Pin<&mut QPainter>, width: i32, height: i32) {
        painter.as_mut().save();
        // qml6glsink centers a fitted rectangle using integer display dimensions.
        let fit = (self.width / f64::from(width)).min(self.height / f64::from(height));
        let display_width = (f64::from(width) * fit).floor().max(1.0);
        let display_height = (f64::from(height) * fit).floor().max(1.0);
        let left = ((self.width - display_width) / 2.0).floor();
        let top = ((self.height - display_height) / 2.0).floor();
        ffi::transform(
            painter.as_mut(),
            0.0,
            0.0,
            f64::from(width) / display_width,
            f64::from(height) / display_height,
        );
        ffi::transform(painter.as_mut(), -left, -top, 1.0, 1.0);
        for layer in &self.layers {
            painter.as_mut().save();
            ffi::transform(painter.as_mut(), layer.x, layer.y, 1.0, 1.0);
            ffi::clip(painter.as_mut(), 0.0, 0.0, layer.width, layer.height);
            for command in &layer.commands {
                match command {
                    Command::Rect {
                        x,
                        y,
                        width,
                        height,
                        color,
                        pose,
                    } => {
                        painter.as_mut().save();
                        if let Some(pose) = pose {
                            pose.apply(painter.as_mut());
                        }
                        ffi::rectangle(painter.as_mut(), *x, *y, *width, *height, &color_of(color));
                        painter.as_mut().restore();
                    }
                    Command::Text(text) => text.paint(painter.as_mut()),
                }
            }
            painter.as_mut().restore();
        }
        painter.as_mut().restore();
        if let Some(image) = &self.media_caption {
            ffi::mediaCaption(painter.as_mut(), image, width, height);
        }
    }
}
fn color_of(value: &str) -> QColor {
    QColor::try_from(value).unwrap_or_else(|_| QColor::from_rgba(0, 0, 0, 0))
}
impl Text {
    fn paint(&self, mut painter: Pin<&mut QPainter>) {
        let mut font = QFont::default();
        font.set_family(&QString::from(self.family.as_str()));
        font.set_pixel_size(self.size.max(1));
        font.set_bold(self.bold);
        font.set_italic(self.italic);
        font.set_underline(self.underline);
        let path = ffi::textPath(
            &QString::from(self.text.as_str()),
            &font,
            self.width,
            self.wrap,
            self.center,
        );
        painter.as_mut().save();
        if let Some(pose) = &self.pose {
            pose.apply(painter.as_mut());
        }
        painter.as_mut().set_opacity(self.opacity.clamp(0.0, 1.0));
        ffi::transform(
            painter.as_mut(),
            self.x + self.width / 2.0,
            self.y + self.baseline,
            self.scale_x,
            1.0,
        );
        ffi::transform(painter.as_mut(), -self.width / 2.0, 0.0, 1.0, 1.0);
        if let Some(shadow) = &self.shadow {
            ffi::shadow(painter.as_mut(), &path, shadow.offset, shadow.radius);
        }
        ffi::path(
            painter.as_mut(),
            &path,
            &color_of(&self.color),
            &color_of(&self.stroke),
            self.stroke_width,
        );
        painter.as_mut().restore();
    }
}
pub fn compose(image: QImage, width: u32, height: u32, overlay: &Overlay) -> Result<QImage, Error> {
    let mut image = if image.width() == width as i32 && image.height() == height as i32 {
        image
    } else {
        image.scaled(
            width as i32,
            height as i32,
            cxx_qt_lib::AspectRatioMode::IgnoreAspectRatio,
            cxx_qt_lib::TransformationMode::SmoothTransformation,
        )
    };
    if image.is_null() {
        return Err(Error::Encode);
    }
    ffi::paintImage(&mut image, overlay);
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotation_uses_the_captured_center_and_keeps_layer_clipping() {
        let overlay = Overlay::parse(
            r##"{"width":100,"height":100,"layers":[
            {"x":0,"y":0,"width":55,"height":100,"commands":[
                {"kind":"rect","x":30,"y":45,"width":40,"height":10,"color":"#ff0000",
                 "pose":{"x":50,"y":50,"degrees":90}}]}]}"##,
        )
        .unwrap();
        let mut image =
            QImage::from_width_height_and_format(100, 100, cxx_qt_lib::QImageFormat::Format_RGB32);
        image.fill(&QColor::from_rgb(0, 0, 0));
        let image = compose(image, 100, 100, &overlay).unwrap();
        assert_eq!(image.pixel_color(50, 35), QColor::from_rgb(255, 0, 0));
        assert_eq!(image.pixel_color(35, 50), QColor::from_rgb(0, 0, 0));
        assert_eq!(image.pixel_color(56, 50), QColor::from_rgb(0, 0, 0));
    }
    #[test]
    fn overlay_maps_from_letterboxed_video_area_and_clips() {
        let overlay = Overlay::parse(
            r##"{"width":100,"height":100,"layers":[
            {"x":0,"y":0,"width":100,"height":100,"commands":[
                {"kind":"rect","x":10,"y":30,"width":20,"height":10,"color":"#ff0000"},
                {"kind":"rect","x":0,"y":0,"width":100,"height":20,"color":"#00ff00"}]}]}"##,
        )
        .unwrap();
        let mut image =
            QImage::from_width_height_and_format(200, 100, cxx_qt_lib::QImageFormat::Format_RGB32);
        image.fill(&QColor::from_rgb(0, 0, 0));
        let image = compose(image, 200, 100, &overlay).unwrap();
        assert_eq!(image.pixel_color(25, 15), QColor::from_rgb(255, 0, 0));
        assert_eq!(image.pixel_color(15, 15), QColor::from_rgb(0, 0, 0));
        assert_eq!(image.pixel_color(25, 35), QColor::from_rgb(0, 0, 0));
        assert_eq!(
            image.pixel_color(0, 0),
            QColor::from_rgb(0, 0, 0),
            "letterbox overlay is outside the saved video"
        );
        // Media captions already use video coordinates, and are above comments
        // in Main.qml. The saved frame must preserve both the fit and this order.
        let mut caption = QImage::from_width_height_and_format(
            200,
            100,
            cxx_qt_lib::QImageFormat::Format_RGBA8888_Premultiplied,
        );
        caption.fill(&QColor::from_rgba(0, 0, 0, 0));
        caption.set_pixel_color(25, 15, &QColor::from_rgb(0, 255, 0));
        let overlay = overlay.with_media_caption(&caption);
        assert_eq!(overlay.media_caption_bytes(), 200 * 100 * 4);
        let image = compose(image, 200, 100, &overlay).unwrap();
        assert_eq!(image.pixel_color(25, 15), QColor::from_rgb(0, 255, 0));
        assert_eq!(image.pixel_color(26, 15), QColor::from_rgb(255, 0, 0));
    }
}
