pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

// One channel and what it is showing now. The owning list decides which card is
// the cursor; the card reports a selection and a favorite request.
ItemDelegate {
    id: card
    required property int channelIndex
    required property string label
    required property string logo
    required property var program
    required property real now
    // "Watch" plays this channel; it keeps the selected surface without focus.
    property bool current: false
    property bool favorite: false
    property bool favoriteEnabled: false
    signal favoriteRequested
    property real feedbackScale: down ? Theme.cardPressScale : highlighted ? Theme.cardFocusScale : 1
    Behavior on feedbackScale {
        NumberAnimation { duration: card.down ? Theme.pressDuration : Theme.moveDuration; easing.type: Easing.OutCubic }
    }
    padding: Theme.spaceLg
    hoverEnabled: true
    Accessible.name: label
    background: CardSurface {
        scale: card.feedbackScale
        selected: card.highlighted || card.current
        hovered: card.hovered
        pressed: card.down
        focused: card.highlighted
    }
    contentItem: Item {
        scale: card.feedbackScale
        RowLayout {
            id: heading
            width: parent.width
            height: Theme.compactControlHeight
            spacing: Theme.spaceSm
            ChannelLogo {
                logoUrl: card.logo || ""
                Layout.preferredWidth: 56
                Layout.preferredHeight: Theme.compactControlHeight
            }
            Label {
                Layout.fillWidth: true
                text: card.label.replace(/^\d+\s+/, "")
                color: Theme.textSecondary
                font.pixelSize: Theme.fontCaption
                textFormat: Text.PlainText
                elide: Text.ElideRight
            }
            FavoriteToggle {
                objectName: "cardFavorite"
                // Align the glyph, not the button, with the card padding.
                Layout.rightMargin: -(implicitWidth - iconSize) / 2
                favorite: card.favorite
                // Stay out of the way until the card is pointed at, focused or already a favorite.
                visible: card.favoriteEnabled && (card.favorite || card.hovered || card.highlighted)
                onFavoriteToggled: card.favoriteRequested()
            }
        }
        ChannelProgram {
            anchors {
                top: heading.bottom; topMargin: Theme.spaceSm
                left: parent.left; right: parent.right; bottom: parent.bottom
            }
            program: card.program
            now: card.now
            emphasized: true
            showRemaining: true
        }
    }
}
