import QtQml
    QtObject {
      readonly property string playbackShortcutLabel: qsTranslate("Settings", "Play or pause")
      readonly property string playbackShortcutCondition: qsTranslate("Settings", "Recordings and timeshift")
      readonly property string commentShortcutCondition: qsTranslate("Settings", "While entering a comment")
      readonly property var day: new Date(2026, 8, 8, 12, 0, 0)
      readonly property string dateLabel: day.toLocaleDateString(Qt.locale(Qt.uiLanguage), qsTranslate("Main", "ddd, MMM d"))
      property string expiredPauseLabel: qsTranslate("Viewer", "Paused outside retained history")
      property string watchingLabel: qsTranslate("Viewer", "Watching · %1").arg("番組A")
      property string broadcastLabel: qsTranslate("Viewer", "On air · %1").arg("番組B")
      property string unknownProgramLabel: qsTranslate("Viewer", "Program information unavailable")
      property string historyLabel: qsTranslate("Viewer", "Outside retained history")
      property string memoryDescription: qsTranslate("Viewer", "Memory keeps rewinding quick without writing to storage. Choose a limit that leaves room for your other apps.")
      property string filesDescription: qsTranslate("Viewer", "Temporary files keep longer history with less RAM. They use storage space and continuous disk writes, and are deleted when playback stops.")
      property string historyEstimate: qsTranslate("Viewer", "About %1 of history").arg(qsTranslate("Viewer", "%1 min %2 sec").arg(1).arg(30))
      property string failureSource: "This channel was not found on Mirakurun. Refresh the channel list and choose a channel again."
      readonly property string failureLabel: qsTranslate("Backend", failureSource)
      property var snapshot: ({state: "Playing"})
      function metric(key) {
        const s = snapshot
        return key === "state" && s.state ? qsTranslate("Backend", s.state) : "—"
      }
      readonly property string stateLabel: metric("state")
      property string heading: qsTr("Stats for nerds"); property string closeLabel: qsTranslate("Main", "Close"); property string emptyChannels: qsTranslate("Viewer", "No matching channels") }
