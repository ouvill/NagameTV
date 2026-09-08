//! Exercise production attachment and teardown with the real, stopped graph.
use super::*;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

/// # Safety
/// The fixture owns both GUI-thread items until this function returns.
pub(crate) unsafe fn check(
    ordinary: *mut crate::player::ffi::QQuickItem,
    video: *mut crate::player::ffi::QQuickItem,
) {
    // Construction and NULL teardown do not open an audio device or stream.
    let mut playback = Playback::new().unwrap();
    let writes = Arc::new(AtomicUsize::new(0));
    let observed = writes.clone();
    playback.sink.connect_notify(Some("widget"), move |_, _| {
        observed.fetch_add(1, Ordering::Relaxed);
    });
    // SAFETY: The fixture supplies live objects; null is explicitly supported.
    unsafe {
        assert!(matches!(
            playback.attach(std::ptr::null_mut()),
            Err(Error::MissingVideoItem)
        ));
        assert!(matches!(
            playback.attach(ordinary),
            Err(Error::InvalidVideoItem)
        ));
        assert_eq!(writes.load(Ordering::Relaxed), 0);
        playback.attach(video).unwrap();
        assert_eq!(writes.load(Ordering::Relaxed), 1);
        assert!(matches!(
            playback.attach(video),
            Err(Error::OutputAlreadyAttached)
        ));
        assert!(matches!(
            playback.attach(ordinary),
            Err(Error::OutputAlreadyAttached)
        ));
        assert_eq!(writes.load(Ordering::Relaxed), 1);
    }
    assert_eq!(
        playback.sink.property::<*mut c_void>("widget"),
        video.cast()
    );

    // Inject each possible transition failure. The real objects stay in NULL;
    // this tests error handling without provoking undefined driver behavior.
    for failure_at in [0, 1] {
        let mut calls = 0;
        let result = playback.shutdown_with(|element| {
            let step = calls;
            calls += 1;
            if step == failure_at {
                Err(gst::StateChangeError)
            } else {
                element.set_state(gst::State::Null).map(|_| ())
            }
        });
        assert!(matches!(result, Err(Error::StateChange(_))));
        assert_eq!(calls, failure_at + 1);
        assert_eq!(playback.video_output, VideoOutputState::Closing);
        assert!(matches!(playback.stop(), Err(Error::OutputShutDown)));
        assert_eq!(writes.load(Ordering::Relaxed), 1);
        assert_eq!(
            playback.sink.property::<*mut c_void>("widget"),
            video.cast()
        );
        // SAFETY: The item is still owned by the synchronous fixture.
        assert!(matches!(
            unsafe { playback.attach(video) },
            Err(Error::OutputShutDown)
        ));
        assert!(matches!(
            playback.play("http://unused.invalid", 1, None),
            Err(Error::OutputNotReady)
        ));
    }

    playback.shutdown().unwrap();
    assert_eq!(playback.video_output, VideoOutputState::Closed);
    assert_eq!(playback.playbin.current_state(), gst::State::Null);
    assert_eq!(playback.sink.current_state(), gst::State::Null);
    assert!(playback.sink.property::<*mut c_void>("widget").is_null());
    assert!(matches!(
        playback.play("http://unused.invalid", 1, None),
        Err(Error::OutputNotReady)
    ));
    // NULL is terminal for this graph (see stop_stream's pad ownership rule).
    // A new output needs a new Playback; repeated shutdown/Drop are harmless.
    assert!(matches!(
        unsafe { playback.attach(video) },
        Err(Error::OutputShutDown)
    ));
    playback.shutdown().unwrap();
    assert_eq!(writes.load(Ordering::Relaxed), 2);
    playback.stop().unwrap();
}
