//! Compatibility guard for fragmented ADTS in GStreamer's AAC parser.
//!
//! Once synchronized, aacparse (including 1.28) can skip an incomplete frame
//! instead of requesting its remaining bytes. In a variable-bitrate broadcast
//! this drops 1024 samples whenever a larger frame crosses a PES boundary.
//! Subclass the installed parser: keep its caps, timestamps, seeking and other
//! AAC formats, and only defer incomplete ADTS frames to the next input buffer.
use glib::translate::*;
use gstreamer::{self as gst, glib, prelude::*};
use gstreamer_base::{self as gst_base, prelude::*};
use std::sync::OnceLock;

const FACTORY: &str = "nagametvaacparse";
// ISO/IEC 13818-7 ADTS fixed header and 13-bit aac_frame_length.
const ADTS_HEADER_BYTES: usize = 7;
const ADTS_CRC_BYTES: usize = 2;
// aacparse uses 10 bytes of lookahead to detect ADTS sync (ADTS_MAX_SIZE).
// Restore that small threshold after a large frame; keeping its full length
// as the minimum would unnecessarily batch subsequent smaller frames.
const SYNC_LOOKAHEAD_BYTES: u32 = 10;
const SYNC_AND_LAYER_MASK: u8 = 0xf6;
const SYNC_AND_LAYER: u8 = 0xf0;
const FREQUENCY_INDEX_MASK: u8 = 0x3c;
const RESERVED_FREQUENCY_INDEX: u8 = 13;
const LENGTH_HIGH_MASK: u8 = 0x03;

type HandleFrame = unsafe extern "C" fn(
    *mut gst_base::ffi::GstBaseParse,
    *mut gst_base::ffi::GstBaseParseFrame,
    *mut std::ffi::c_int,
) -> gst::ffi::GstFlowReturn;
static PARENT_HANDLE_FRAME: OnceLock<HandleFrame> = OnceLock::new();
static REGISTERED: OnceLock<Result<(), glib::BoolError>> = OnceLock::new();

pub(super) fn register() -> Result<(), glib::BoolError> {
    REGISTERED
        .get_or_init(|| {
            let factory = gst::ElementFactory::find("aacparse")
                .ok_or_else(|| glib::bool_error!("AAC parser is unavailable"))?
                .load()?
                .downcast::<gst::ElementFactory>()
                .map_err(|_| glib::bool_error!("AAC parser factory has an invalid type"))?;
            let parent = factory.element_type();
            // SAFETY: factory.load() registered this live GType. A future
            // plugin may make it final; reject that ABI before subclassing it.
            if unsafe {
                glib::gobject_ffi::g_type_test_flags(
                    parent.into_glib(),
                    glib::gobject_ffi::G_TYPE_FLAG_FINAL,
                )
            } != 0
            {
                return Err(glib::bool_error!(
                    "AAC parser no longer supports subclassing"
                ));
            }
            let class = glib::Class::<gst_base::BaseParse>::from_type(parent)
                .ok_or_else(|| glib::bool_error!("AAC parser is not a GstBaseParse"))?;
            // The loaded class was checked as a GstBaseParse above.
            // Only the public base-class vfunc is inspected; no plugin-private
            // fields or compile-time assumptions about its size are used.
            let callback = class
                .as_ref()
                .handle_frame
                .ok_or_else(|| glib::bool_error!("AAC parser has no frame handler"))?;
            PARENT_HANDLE_FRAME
                .set(callback)
                .map_err(|_| glib::bool_error!("AAC parser guard was already initialized"))?;
            // SAFETY: GLib supplies the actual parent class/instance sizes.
            // This subclass adds no fields, inherits parent initialization and
            // overrides only GstBaseParseClass::handle_frame in its own class.
            let type_ = unsafe {
                let mut query = std::mem::zeroed();
                glib::gobject_ffi::g_type_query(parent.into_glib(), &mut query);
                from_glib(glib::gobject_ffi::g_type_register_static_simple(
                    parent.into_glib(),
                    c"NagameTvAacParse".as_ptr(),
                    query.class_size,
                    Some(class_init),
                    query.instance_size,
                    None,
                    0,
                ))
            };
            gst::Element::register(None, FACTORY, factory.rank() + 1, type_)
        })
        .clone()
}

unsafe extern "C" fn class_init(class: glib::ffi::gpointer, _: glib::ffi::gpointer) {
    // SAFETY: registered directly below the validated AAC GstBaseParse class.
    unsafe {
        (*class.cast::<gst_base::ffi::GstBaseParseClass>()).handle_frame = Some(handle_frame)
    };
}

fn incomplete_adts(bytes: &[u8]) -> Option<u32> {
    let header = bytes.get(..ADTS_HEADER_BYTES)?;
    if header[0] != 0xff
        || header[1] & SYNC_AND_LAYER_MASK != SYNC_AND_LAYER
        || (header[2] & FREQUENCY_INDEX_MASK) >> 2 >= RESERVED_FREQUENCY_INDEX
    {
        return None;
    }
    let length = (usize::from(header[3] & LENGTH_HIGH_MASK) << 11)
        | (usize::from(header[4]) << 3)
        | usize::from(header[5] >> 5);
    let header_length = ADTS_HEADER_BYTES
        + if header[1] & 1 == 0 {
            ADTS_CRC_BYTES
        } else {
            0
        };
    (length >= header_length && length > bytes.len()).then_some(length as u32)
}

unsafe extern "C" fn handle_frame(
    parse: *mut gst_base::ffi::GstBaseParse,
    frame: *mut gst_base::ffi::GstBaseParseFrame,
    skip: *mut std::ffi::c_int,
) -> gst::ffi::GstFlowReturn {
    // SAFETY: GstBaseParse invokes this vfunc with live, borrowed arguments
    // under its streaming lock. No reference escapes this call.
    let parser: Borrowed<gst_base::BaseParse> = unsafe { from_glib_borrow(parse) };
    let buffer = unsafe { gst::BufferRef::from_ptr((*frame).buffer) };
    let adts = parser.sink_pad().current_caps().is_some_and(|caps| {
        caps.structure(0)
            .is_some_and(|s| s.get::<&str>("stream-format") == Ok("adts"))
    });
    if adts && !parser.is_draining() {
        let data = match buffer.map_readable() {
            Ok(data) => data,
            Err(error) => {
                tracing::error!(%error, "Failed to inspect fragmented AAC frame");
                return gst::ffi::GST_FLOW_ERROR;
            }
        };
        if let Some(length) = incomplete_adts(data.as_slice()) {
            parser.set_min_frame_size(length);
            // SAFETY: writable out parameter provided by GstBaseParse.
            unsafe { *skip = 0 };
            return gst::ffi::GST_FLOW_OK;
        }
        parser.set_min_frame_size(SYNC_LOOKAHEAD_BYTES);
    }
    match PARENT_HANDLE_FRAME.get() {
        // SAFETY: inherited handler, saved before this type can be constructed;
        // the subclass includes the complete parent instance and class layout.
        Some(parent) => unsafe { parent(parse, frame, skip) },
        None => {
            tracing::error!("AAC parser guard has no parent frame handler");
            gst::ffi::GST_FLOW_ERROR
        }
    }
}

#[cfg(test)]
mod tests;
