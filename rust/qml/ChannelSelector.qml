import QtQuick
import MinimalViewer
import QtQuick.Layouts

RowLayout {
    id: root
    required property ChannelModel channels
    required property int selected
    signal selectRequested(int index)
    property string band: "ALL"
    ChannelFilterModel { id: channelFilter; sourceModel: root.channels; band: root.band }

    // Changing a view filter never starts playback. Previous/next and restored
    // selections outside this filter reveal the selected channel in the full list.
    onSelectedChanged: {
        if (selected >= 0 && selected < channels.count && band !== "ALL" && channels.row(selected).band !== band)
            band = "ALL"
    }
    SettingsChoice {
        objectName: "bandSelector"
        Layout.preferredWidth: 90
        model: [
            { label: qsTranslate("Viewer", "All"), value: "ALL" },
            { label: qsTranslate("Main", "Terrestrial"), value: "GR" },
            { label: "BS", value: "BS" },
            { label: "CS", value: "CS" },
            { label: "SKY", value: "SKY" },
            { label: qsTranslate("Viewer", "Other"), value: "OTHER" }
        ]
        textRole: "label"
        valueRole: "value"
        currentValue: root.band
        onActivated: root.band = currentValue
    }
    SettingsChoice {
        objectName: "channelSelector"
        Layout.fillWidth: true
        model: channelFilter
        textRole: "label"
        valueRole: "channelIndex"
        currentValue: root.selected
        displayText: currentIndex >= 0 ? currentText : (count === 0 ? qsTranslate("Viewer", "No matching channels") : qsTranslate("Viewer", "Choose a channel"))
        enabled: count > 0
        onActivated: root.selectRequested(currentValue)
    }
}
