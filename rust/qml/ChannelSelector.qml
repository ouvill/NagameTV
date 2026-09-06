import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

RowLayout {
    id: root
    required property var rows
    required property int selected
    signal selectRequested(int index)
    property string band: "ALL"
    readonly property var filteredRows: band === "ALL" ? rows : rows.filter(row => row.band === band)

    // Changing a view filter never starts playback. Previous/next and restored
    // selections outside this filter reveal the selected channel in the full list.
    onSelectedChanged: {
        if (selected >= 0 && selected < rows.length && band !== "ALL" && rows[selected].band !== band)
            band = "ALL"
    }
    ComboBox {
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
    ComboBox {
        objectName: "channelSelector"
        Layout.fillWidth: true
        model: root.filteredRows
        textRole: "label"
        valueRole: "index"
        currentValue: root.selected
        displayText: currentIndex >= 0 ? currentText : (count === 0 ? qsTranslate("Viewer", "No matching channels") : qsTranslate("Viewer", "Choose a channel"))
        enabled: count > 0
        onActivated: root.selectRequested(currentValue)
    }
}
