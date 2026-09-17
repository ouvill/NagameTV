pragma ComponentBehavior: Bound
import QtQuick

SegmentedControl {
    id: root
    required property var rows
    property string uiLanguage: Qt.uiLanguage
    objectNamePrefix: "band-"
    directionalNavigation: false
    options: [
        {
            value: "GR",
            label: qsTranslate("Main", "Terrestrial")
        },
        {
            value: "BS",
            label: "BS"
        },
        {
            value: "CS",
            label: "CS"
        },
        {
            value: "SKY",
            label: "SKY"
        },
        {
            value: "OTHER",
            label: qsTranslate("Viewer", "Other")
        }
    ].filter(option => rows.some(row => row.band === option.value))
    implicitWidth: Math.max(82, options.length * (uiLanguage === "en" ? 96 : 76) + 6)
    visible: options.length > 0
}
