import QtQuick

IconAction {
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    iconSource: iconDirectory + "chevron-down.svg"
    tip: qsTranslate("Main", "Collapse")
    Accessible.name: qsTranslate("Viewer", "Close channel selection")
}
