import QtQuick
Item {
    id: observer
    objectName: "liveTimelineObserver"
    visible: false
    property var backend: null
    property string failure: ""
    property int notifications: 0
    property var saved: null
    property string previousSession: ""
    property int seekRequests: 0
    property string lastSeekSession: ""
    property real lastSeekTarget: -1
    property real lastSeekEdge: -1
    property real lastSeekPreviousEdge: -1
    property string observedSession: ""
    property real previousLivePosition: -1
    property bool programAcquired: false
    function check() {
        const model = JSON.parse(backend.live_timeline);
        if (!model) return;
        notifications++;
        if (observedSession !== model.session) {
            observedSession = model.session;
            previousLivePosition = -1;
            programAcquired = false;
        }
        if (model.seekTarget !== null
                && (lastSeekSession !== model.session || lastSeekTarget !== model.seekTarget)) {
            seekRequests++;
            lastSeekSession = model.session;
            lastSeekTarget = model.seekTarget;
            lastSeekEdge = model.live.position;
            lastSeekPreviousEdge = previousLivePosition;
        }
        previousLivePosition = model.live.position;
        const program = model.viewing && model.viewing.program;
        // The continuous fixture carries EIT throughout both programs. Catch
        // even a single notification that clears an already acquired title.
        if (programAcquired && !program && model.state === "playing")
            failure = "Live program disappeared during playback: " + JSON.stringify(model);
        if (program)
            programAcquired = true;
        const expected = program ? program.data : null;
        if (JSON.stringify(JSON.parse(backend.current_program_data)) !== JSON.stringify(expected))
            failure = "Program notification exposed different viewing metadata";
        if (backend.program_progress !== (program ? program.progress : 0))
            failure = "Program notification exposed a different viewing progress";
        const live = model.live.program;
        if (live && live.span && model.axis.end === live.span.end
                && model.axis.endUtc !== live.data.startAt + live.data.duration)
            failure = "Program end clock differs from the broadcast schedule";
        if ((model.state === "seeking") !== backend.seeking || (model.state === "paused" && !backend.paused))
            failure = "Transport notification exposed a different timeline state";
    }
    Connections {
        target: observer.backend
        function onLive_timelineChanged() { observer.check(); }
        function onCurrent_program_dataChanged() { observer.check(); }
        function onProgram_progressChanged() { observer.check(); }
        function onPausedChanged() { observer.check(); }
        function onSeekingChanged() { observer.check(); }
        function onTimeshiftChanged() { observer.check(); }
    }
}
