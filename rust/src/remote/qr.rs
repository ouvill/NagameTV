//! QR encoding of the local browser URL; no image files or native resources.
#![forbid(unsafe_code)]
use base64::Engine;

pub fn data_uri(url: &str) -> Result<String, qrcode::types::QrError> {
    let code = qrcode::QrCode::new(url.as_bytes())?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(256, 256)
        .quiet_zone(true)
        .build();
    Ok(format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(svg)
    ))
}
