//! Presentation only: the core produces premultiplied pixels at the video clock.
use cxx_qt::CxxQtType;
use cxx_qt_lib::QImage;
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("media_caption.h");
        type QQuickPaintedItem;
        type QPainter;
        include!("cxx-qt-lib/qimage.h");
        type QImage = cxx_qt_lib::QImage;
        #[cxx_name = "paintMediaCaption"]
        unsafe fn paint_media_caption(
            painter: *mut QPainter,
            image: &QImage,
            width: f64,
            height: f64,
        );
    }
    impl cxx_qt::Initialize for MediaCaption {}
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickPaintedItem]
        #[qproperty(QImage, image, READ, WRITE = set_image, NOTIFY)]
        type MediaCaption = super::Caption;
        fn set_image(self: Pin<&mut MediaCaption>, image: QImage);
        #[cxx_override]
        unsafe fn paint(self: Pin<&mut MediaCaption>, painter: *mut QPainter);
        #[inherit]
        fn update(self: Pin<&mut MediaCaption>);
        #[inherit]
        fn width(self: &MediaCaption) -> f64;
        #[inherit]
        fn height(self: &MediaCaption) -> f64;
    }
}
#[derive(Default)]
pub struct Caption {
    image: QImage,
}
impl ffi::MediaCaption {
    pub fn set_image(mut self: Pin<&mut Self>, image: QImage) {
        self.as_mut().rust_mut().image = image;
        self.as_mut().image_changed();
        self.update();
    }
    /// # Safety
    /// Qt lends a painter only for this paint callback.
    pub unsafe fn paint(self: Pin<&mut Self>, painter: *mut ffi::QPainter) {
        unsafe {
            ffi::paint_media_caption(painter, &self.rust().image, self.width(), self.height())
        }
    }
}

impl cxx_qt::Initialize for ffi::MediaCaption {
    fn initialize(self: Pin<&mut Self>) {}
}
