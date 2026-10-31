pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer

SegmentedControl {
    id: root
    required property ChannelModel channels
    property string uiLanguage: Qt.uiLanguage
    objectNamePrefix: "band-"
    directionalNavigation: false
    // Home lists favorites across bands as the first tab.
    property bool favorites: false
    options: {
        channels.revision;
        return (favorites ? [{ value: "FAV", label: qsTranslate("Viewer", "Favorites") }] : []).concat([
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
        ].filter(option => channels.has_band(option.value)));
    }
    implicitWidth: Math.max(82, options.length * (uiLanguage === "en" ? 96 : 76) + 6 + (favorites ? 24 : 0))
    visible: options.length > 0
}
