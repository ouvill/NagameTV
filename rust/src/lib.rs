use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr::{self, NonNull};
use std::sync::Mutex;

#[repr(C)]
struct MpvHandle {
    _private: [u8; 0],
}

#[repr(C)]
struct MpvRenderContext {
    _private: [u8; 0],
}

#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct MpvRenderParam {
    kind: c_int,
    data: *mut c_void,
}

type GlGetProcAddress = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;

#[repr(C)]
struct MpvOpenGlInitParams {
    get_proc_address: Option<GlGetProcAddress>,
    get_proc_address_ctx: *mut c_void,
}

#[repr(C)]
struct MpvOpenGlFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

#[link(name = "mpv")]
extern "C" {
    fn mpv_create() -> *mut MpvHandle;
    fn mpv_initialize(handle: *mut MpvHandle) -> c_int;
    fn mpv_terminate_destroy(handle: *mut MpvHandle);
    fn mpv_set_option_string(handle: *mut MpvHandle, name: *const c_char,
                             value: *const c_char) -> c_int;
    fn mpv_set_property_string(handle: *mut MpvHandle, name: *const c_char,
                               value: *const c_char) -> c_int;
    fn mpv_command(handle: *mut MpvHandle, args: *const *const c_char) -> c_int;
    fn mpv_wait_event(handle: *mut MpvHandle, timeout: f64) -> *const MpvEvent;
    fn mpv_error_string(error: c_int) -> *const c_char;
    fn mpv_render_context_create(result: *mut *mut MpvRenderContext,
                                 handle: *mut MpvHandle,
                                 params: *mut MpvRenderParam) -> c_int;
    fn mpv_render_context_render(context: *mut MpvRenderContext,
                                 params: *mut MpvRenderParam) -> c_int;
    fn mpv_render_context_report_swap(context: *mut MpvRenderContext);
    fn mpv_render_context_free(context: *mut MpvRenderContext);
}

const MPV_EVENT_NONE: c_int = 0;
const MPV_EVENT_SHUTDOWN: c_int = 1;
const MPV_EVENT_END_FILE: c_int = 7;
const MPV_EVENT_FILE_LOADED: c_int = 8;
const MPV_RENDER_PARAM_API_TYPE: c_int = 1;
const MPV_RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
const MPV_RENDER_PARAM_OPENGL_FBO: c_int = 3;
const MPV_RENDER_PARAM_FLIP_Y: c_int = 4;

pub struct MirakurunPlayer {
    handle: NonNull<MpvHandle>,
    renderer: Mutex<Option<NonNull<MpvRenderContext>>>,
}

// libmpv's client API is thread-safe. Access to its render context is serialized
// separately because Qt's UI and render threads are distinct.
unsafe impl Send for MirakurunPlayer {}
unsafe impl Sync for MirakurunPlayer {}

impl MirakurunPlayer {
    fn new() -> Result<Self, String> {
        let handle = NonNull::new(unsafe { mpv_create() })
            .ok_or_else(|| "libmpv のハンドルを作成できませんでした".to_owned())?;

        let player = Self { handle, renderer: Mutex::new(None) };
        for (name, value) in [
            ("terminal", "no"),
            ("config", "no"),
            ("input-default-bindings", "no"),
            ("input-vo-keyboard", "no"),
            ("keep-open", "yes"),
            ("idle", "yes"),
            ("cache", "yes"),
            ("cache-secs", "5"),
            ("demuxer-max-bytes", "64MiB"),
            ("demuxer-max-back-bytes", "0"),
            ("network-timeout", "10"),
            ("stream-lavf-o", "reconnect=1,reconnect_streamed=1,reconnect_delay_max=5"),
            ("hwdec", "auto-safe"),
            ("deinterlace", "yes"),
        ] {
            player.set_option(name, value)?;
        }
        check(unsafe { mpv_initialize(player.handle.as_ptr()) })?;
        player.set_property("volume", "70")?;
        Ok(player)
    }

    fn set_option(&self, name: &str, value: &str) -> Result<(), String> {
        let name = cstring(name)?;
        let value = cstring(value)?;
        check(unsafe { mpv_set_option_string(self.handle.as_ptr(), name.as_ptr(), value.as_ptr()) })
    }

    fn set_property(&self, name: &str, value: &str) -> Result<(), String> {
        let name = cstring(name)?;
        let value = cstring(value)?;
        check(unsafe { mpv_set_property_string(self.handle.as_ptr(), name.as_ptr(), value.as_ptr()) })
    }

    fn command(&self, values: &[&str]) -> Result<(), String> {
        let strings: Result<Vec<_>, _> = values.iter().map(|value| cstring(value)).collect();
        let strings = strings?;
        let mut args: Vec<_> = strings.iter().map(|value| value.as_ptr()).collect();
        args.push(ptr::null());
        check(unsafe { mpv_command(self.handle.as_ptr(), args.as_ptr()) })
    }

    fn play_service(&self, server: &str, service_id: u64) -> Result<(), String> {
        let url = service_stream_url(server, service_id)?;
        self.command(&["loadfile", &url, "replace"])
    }

    fn init_renderer(&self, get_proc: GlGetProcAddress,
                     get_proc_context: *mut c_void) -> Result<(), String> {
        let mut guard = self.renderer.lock().map_err(|_| "描画ロックが破損しました".to_owned())?;
        if guard.is_some() {
            return Ok(());
        }

        let api = b"opengl\0";
        let mut init = MpvOpenGlInitParams {
            get_proc_address: Some(get_proc),
            get_proc_address_ctx: get_proc_context,
        };
        let mut params = [
            MpvRenderParam { kind: MPV_RENDER_PARAM_API_TYPE,
                data: api.as_ptr() as *mut c_void },
            MpvRenderParam { kind: MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                data: (&mut init as *mut MpvOpenGlInitParams).cast() },
            MpvRenderParam { kind: 0, data: ptr::null_mut() },
        ];
        let mut context = ptr::null_mut();
        check(unsafe { mpv_render_context_create(&mut context, self.handle.as_ptr(), params.as_mut_ptr()) })?;
        *guard = NonNull::new(context);
        Ok(())
    }

    fn render(&self, framebuffer: c_int, width: c_int, height: c_int) -> bool {
        if width <= 0 || height <= 0 {
            return false;
        }
        let guard = match self.renderer.lock() {
            Ok(value) => value,
            Err(_) => return false,
        };
        let Some(context) = *guard else { return false };
        let mut fbo = MpvOpenGlFbo { fbo: framebuffer, w: width, h: height, internal_format: 0 };
        let mut flip: c_int = 1;
        let mut params = [
            MpvRenderParam { kind: MPV_RENDER_PARAM_OPENGL_FBO,
                data: (&mut fbo as *mut MpvOpenGlFbo).cast() },
            MpvRenderParam { kind: MPV_RENDER_PARAM_FLIP_Y,
                data: (&mut flip as *mut c_int).cast() },
            MpvRenderParam { kind: 0, data: ptr::null_mut() },
        ];
        let result = unsafe { mpv_render_context_render(context.as_ptr(), params.as_mut_ptr()) };
        if result >= 0 {
            unsafe { mpv_render_context_report_swap(context.as_ptr()) };
        }
        result >= 0
    }

    fn free_renderer(&self) {
        if let Ok(mut guard) = self.renderer.lock() {
            if let Some(context) = guard.take() {
                unsafe { mpv_render_context_free(context.as_ptr()) };
            }
        }
    }
}
impl Drop for MirakurunPlayer {
    fn drop(&mut self) {
        self.free_renderer();
        unsafe { mpv_terminate_destroy(self.handle.as_ptr()) };
    }
}

fn service_stream_url(server: &str, service_id: u64) -> Result<String, String> {
    let server = server.trim().trim_end_matches('/');
    if !(server.starts_with("http://") || server.starts_with("https://")) {
        return Err("サーバーURLは http:// または https:// で始めてください".to_owned());
    }
    if service_id == 0 {
        return Err("service ID は1以上である必要があります".to_owned());
    }
    Ok(format!("{server}/api/services/{service_id}/stream"))
}

fn cstring(value: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| "文字列にNUL文字が含まれています".to_owned())
}

fn check(code: c_int) -> Result<(), String> {
    if code >= 0 { return Ok(()) }
    let message = unsafe {
        let ptr = mpv_error_string(code);
        if ptr.is_null() { "unknown libmpv error".to_owned() }
        else { CStr::from_ptr(ptr).to_string_lossy().into_owned() }
    };
    Err(message)
}

unsafe fn input_string<'a>(value: *const c_char) -> Result<&'a str, String> {
    if value.is_null() { return Err("NULL文字列を受け取りました".to_owned()) }
    CStr::from_ptr(value).to_str().map_err(|_| "UTF-8として解釈できません".to_owned())
}

unsafe fn write_error(buffer: *mut c_char, capacity: usize, message: &str) {
    if buffer.is_null() || capacity == 0 { return }
    let bytes = message.as_bytes();
    let length = bytes.len().min(capacity - 1);
    ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast(), length);
    *buffer.add(length) = 0;
}

#[no_mangle]
pub unsafe extern "C" fn mirakurun_player_create(error: *mut c_char, capacity: usize)
    -> *mut MirakurunPlayer {
    match MirakurunPlayer::new() {
        Ok(player) => Box::into_raw(Box::new(player)),
        Err(message) => { write_error(error, capacity, &message); ptr::null_mut() }
    }
}

#[no_mangle]
pub unsafe extern "C" fn mirakurun_player_destroy(player: *mut MirakurunPlayer) {
    if !player.is_null() { drop(Box::from_raw(player)); }
}

#[no_mangle]
pub unsafe extern "C" fn mirakurun_player_play_service(
    player: *mut MirakurunPlayer, server: *const c_char, service_id: u64,
    error: *mut c_char, capacity: usize) -> bool {
    let result = (|| (*player.as_ref().ok_or("プレイヤーがNULLです")?)
        .play_service(input_string(server)?, service_id))();
    match result {
        Ok(()) => true,
        Err(message) => { write_error(error, capacity, &message); false }
    }
}

#[no_mangle]
pub unsafe extern "C" fn mirakurun_player_stop(player: *mut MirakurunPlayer) {
    if let Some(player) = player.as_ref() { let _ = player.command(&["stop"]); }
}

#[no_mangle]
pub unsafe extern "C" fn mirakurun_player_set_pause(player: *mut MirakurunPlayer, paused: bool) {
    if let Some(player) = player.as_ref() { let _ = player.set_property("pause", if paused { "yes" } else { "no" }); }
}

#[no_mangle]
pub unsafe extern "C" fn mirakurun_player_set_volume(player: *mut MirakurunPlayer, volume: f64) {
    if let Some(player) = player.as_ref() { let _ = player.set_property("volume", &volume.clamp(0.0, 100.0).to_string()); }
}

#[no_mangle]
pub unsafe extern "C" fn mirakurun_player_drain_events(player: *mut MirakurunPlayer) -> c_int {
    let Some(player) = player.as_ref() else { return 3 };
    let mut outcome = 0;
    loop {
        let event = mpv_wait_event(player.handle.as_ptr(), 0.0);
        if event.is_null() || (*event).event_id == MPV_EVENT_NONE { break }
        outcome = match (*event).event_id {
            MPV_EVENT_FILE_LOADED => 1,
            MPV_EVENT_END_FILE => 2,
            MPV_EVENT_SHUTDOWN => 3,
            _ => outcome,
        };
    }
    outcome
}

#[no_mangle]
pub unsafe extern "C" fn mirakurun_player_init_renderer(
    player: *mut MirakurunPlayer, get_proc: Option<GlGetProcAddress>, context: *mut c_void,
    error: *mut c_char, capacity: usize) -> bool {
    let result = match (player.as_ref(), get_proc) {
        (Some(player), Some(get_proc)) => player.init_renderer(get_proc, context),
        _ => Err("描画初期化引数がNULLです".to_owned()),
    };
    match result {
        Ok(()) => true,
        Err(message) => { write_error(error, capacity, &message); false }
    }
}

#[no_mangle]
pub unsafe extern "C" fn mirakurun_player_render(
    player: *mut MirakurunPlayer, framebuffer: c_int, width: c_int, height: c_int) -> bool {
    player.as_ref().map_or(false, |player| player.render(framebuffer, width, height))
}

#[no_mangle]
pub unsafe extern "C" fn mirakurun_player_free_renderer(player: *mut MirakurunPlayer) {
    if let Some(player) = player.as_ref() { player.free_renderer(); }
}

#[cfg(test)]
mod tests {
    use super::service_stream_url;

    #[test]
    fn builds_mirakurun_service_url() {
        assert_eq!(service_stream_url("http://localhost:40772/", 1024).unwrap(),
                   "http://localhost:40772/api/services/1024/stream");
    }

    #[test]
    fn rejects_unsafe_scheme_and_zero_id() {
        assert!(service_stream_url("file:///tmp", 1).is_err());
        assert!(service_stream_url("http://localhost", 0).is_err());
    }
}
