import QtQml
    QtObject {
      readonly property var day: new Date(2026, 8, 8, 12, 0, 0)
      readonly property string dateLabel: day.toLocaleDateString(Qt.locale(Qt.uiLanguage), qsTranslate("Main", "ddd, MMM d"))
      property string failureSource: "This channel was not found on Mirakurun. Refresh the channel list and choose a channel again."
      readonly property string failureLabel: qsTranslate("Backend", failureSource)
      property var snapshot: ({state: "Playing"})
      function metric(key) {
        const s = snapshot
        return key === "state" && s.state ? qsTranslate("Backend", s.state) : "—"
      }
      readonly property string stateLabel: metric("state")
      property string heading: qsTr("Stats for nerds"); property string closeLabel: qsTranslate("Main", "Close"); property string emptyChannels: qsTranslate("Viewer", "No matching channels") }
