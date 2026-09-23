pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import MinimalViewer

// Development catalogue; uses production components with local sample values.
Rectangle {
    id: gallery
    color: Theme.surface
    implicitWidth: 1280
    implicitHeight: 720
    Item {
        width: 1280
        height: 720
        scale: Math.min(gallery.width / width, gallery.height / height)
        transformOrigin: Item.TopLeft
        ColumnLayout {
            anchors { fill: parent; margins: Theme.spaceXl }
            spacing: Theme.spaceLg
            Label {
                text: "共通部品 / Controls"
                color: Theme.textPrimary
                font.pixelSize: Theme.fontTitle
                font.bold: true
            }
            Label {
                text: "通常・押下・選択・無効。ポインターと Tab / Space / 矢印キーでも操作できます。"
                color: Theme.textSecondary
                font.pixelSize: Theme.fontBody
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceXl
                Repeater {
                    model: ["通常 / Normal", "押下 / Pressed", "選択 / Selected", "無効 / Disabled"]
                    ColumnLayout {
                        id: sample
                        required property int index
                        required property string modelData
                        Layout.fillWidth: true
                        spacing: Theme.spaceSm
                        Label { text: sample.modelData; color: Theme.textSecondary; font.pixelSize: Theme.fontBody }
                        Repeater {
                            model: [ActionButton.Primary, ActionButton.Secondary, ActionButton.Quiet]
                            ActionButton {
                                required property int modelData
                                objectName: "button_" + sample.index + "_" + modelData
                                Layout.fillWidth: true
                                emphasis: modelData
                                text: modelData === ActionButton.Primary ? "主操作 / Primary"
                                    : modelData === ActionButton.Secondary ? "補助操作 / Secondary" : "控えめ / Quiet"
                                down: sample.index === 1 || pressed
                                selected: sample.index === 2
                                enabled: sample.index !== 3
                            }
                        }
                        RowLayout {
                            IconAction {
                                objectName: "icon_" + sample.index
                                iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/play.svg"
                                tip: "再生 / Play"
                                down: sample.index === 1 || pressed
                                active: sample.index === 2
                                enabled: sample.index !== 3
                                flat: true
                            }
                            ActionButton {
                                surface: ActionButton.VideoOverlay
                                text: "x1.0"
                                down: sample.index === 1 || pressed
                                selected: sample.index === 2
                                enabled: sample.index !== 3
                            }
                            ToggleSwitch {
                                objectName: "toggle_" + sample.index
                                text: "切り替え / Toggle"
                                down: sample.index === 1 || pressed
                                checked: sample.index === 2
                                enabled: sample.index !== 3
                            }
                        }
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceXl
                ColumnLayout {
                    Layout.fillWidth: true
                    SettingsField { objectName: "field"; Layout.fillWidth: true; placeholderText: "入力欄 / Text field" }
                    SettingsField { Layout.fillWidth: true; text: "無効 / Disabled"; enabled: false }
                    SettingsToggle { Layout.fillWidth: true; text: "設定項目 / Setting"; description: "選択に必要な説明" }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    SettingsChoice { objectName: "choice"; Layout.fillWidth: true; model: ["選択肢 / Option A", "選択肢 / Option B"] }
                    ThemedSpinBox { objectName: "number"; Layout.fillWidth: true; from: 0; to: 100; value: 50 }
                    ThemedSlider { objectName: "slider"; Layout.fillWidth: true; from: 0; to: 100; value: 50; stepSize: 10 }
                }
            }
            SegmentedControl {
                objectName: "segments"
                Layout.fillWidth: true
                options: [{value: "a", label: "ライブ / Live"}, {value: "b", label: "録画 / Recording"}]
                value: "a"
                onSelected: function(value) { this.value = value; }
            }
            PanelSurface {
                Layout.fillWidth: true
                implicitHeight: 80
                RowLayout {
                    anchors { fill: parent; margins: Theme.spaceLg }
                    Label { text: "ポップアップ / Popup"; color: Theme.textPrimary; font.pixelSize: Theme.fontHeading }
                    Item { Layout.fillWidth: true }
                    Label { text: "補助情報"; color: Theme.textSecondary; font.pixelSize: Theme.fontBody }
                    Label { text: "注意"; color: Theme.warning; font.pixelSize: Theme.fontBody }
                    Label { text: "エラー"; color: Theme.error; font.pixelSize: Theme.fontBody }
                }
            }
            Item { Layout.fillHeight: true }
        }
    }
}
