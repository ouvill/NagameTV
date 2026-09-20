import QtQuick
import MinimalViewer 1.0

// Only records commands; Rust tests own playback state transitions.
QtObject {
    property int playback_rate: 10
    property int requested_playback_rate: 10
    readonly property int minimum_playback_rate: 5
    readonly property int maximum_playback_rate: 20
    property bool speed_available: true
    property string speed_reason: ""
    property string transport_error: ""
    property string recording_name: ""
    property bool rateAccepted: true
    property var rateRequests: []
    property bool at_live_edge: !recording && media_active && !paused && !seeking && (!timeshift || live_delay_ms <= 1250)
    function set_playback_rate(value) {
        rateRequests.push(value);
        if (rateAccepted) requested_playback_rate = value;
        return rateAccepted;
    }
    property bool playing: false
    property bool paused: false
    property bool seeking: false
    property real live_delay_ms: 0
    property int liveRequests: 0
    function return_to_live() { liveRequests++; }
    property bool recording: false
    property bool timeshift: false
    property bool media_active: playing || paused
    property bool connecting: false
    property bool seekable: (recording || timeshift) && media_active
    property int playback_action: Player.Play
    property int selected: 0
    property bool audio_muted: false
    property real volume_level: 0.5
    property bool comments_enabled: true
    property bool danmaku_enabled: false
    property real comment_font_size: 24
    property real comment_opacity: 0.8
    property real comment_speed: 1.2
    property bool subtitles_enabled: true
    property bool subtitle_display: true
    property bool epg_enabled: true
    property bool guide_visible: false
    property int saved: 0
    property int playbackRequests: 0
    property int channelSteps: 0
    property var skips: []
    function toggle_playback() { playbackRequests++; }
    function skip(milliseconds) { skips.push(milliseconds); }
    function step_channel(offset) { channelSteps += offset; }
    function guide_open(visible) { guide_visible = visible; }
    function mute(value) { audio_muted = value; }
    function volume(value) { volume_level = value; }
    function save_settings() { saved++; }
    function display_subtitles(value) { subtitle_display = value; }
    function configure_danmaku(value, size, opacity, speed) { danmaku_enabled = value; }
}
