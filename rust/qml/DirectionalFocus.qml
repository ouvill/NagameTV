import QtQuick
import QtQuick.Templates as Templates

// Only explicitly listed controls participate. Lists retain their own cursor
// navigation; a screen connects the boundaries between these small regions.
FocusScope {
    id: root
    property list<Item> navigationItems
    property Item initialItem: null
    readonly property Item currentItem: navigationItems.find(item => item && item.activeFocus) || null
    readonly property Templates.Control currentControl: currentItem as Templates.Control
    readonly property bool navigating: activeFocus && (currentItem instanceof IconAction
        ? (currentItem as IconAction).focusVisible : currentItem instanceof ActionButton
        ? (currentItem as ActionButton).focusVisible : currentControl !== null && currentControl.visualFocus)
    signal boundaryReached(int key)
    QtObject { id: memory; property Item item: null }
    onCurrentItemChanged: {
        if (navigationItems.includes(currentItem)) memory.item = currentItem;
    }
    onActiveFocusChanged: if (activeFocus) Qt.callLater(function() {
        if (root.activeFocus && !root.currentItem) root.enter();
    })
    function available(item: Item): bool {
        return item !== null && item.visible && item.enabled;
    }
    function enter() {
        const target = available(memory.item) ? memory.item : available(initialItem) ? initialItem
            : navigationItems.find(item => available(item));
        if (target) target.forceActiveFocus(Qt.TabFocusReason);
    }
    function move(key: int) {
        if (!navigationItems.includes(currentItem)) { enter(); return; }
        const horizontal = key === Qt.Key_Left || key === Qt.Key_Right;
        const sign = key === Qt.Key_Left || key === Qt.Key_Up ? -1 : 1;
        const origin = currentItem.mapToItem(root, currentItem.width / 2, currentItem.height / 2);
        let next = null;
        let bestDistance = Infinity;
        for (const item of navigationItems) {
            if (item === currentItem || !available(item)) continue;
            const point = item.mapToItem(root, item.width / 2, item.height / 2);
            const along = (horizontal ? point.x - origin.x : point.y - origin.y) * sign;
            const across = Math.abs(horizontal ? point.y - origin.y : point.x - origin.x);
            // Horizontal moves stay in their row, including stacked controls.
            const sameRow = across < (currentItem.height + item.height) / 2;
            if (along <= 0 || (horizontal && !sameRow)) continue;
            const distance = along * along + across * across;
            if (distance < bestDistance) { next = item; bestDistance = distance; }
        }
        if (next) next.forceActiveFocus(Qt.TabFocusReason);
        else boundaryReached(key);
    }
    Keys.onPressed: function(event) {
        if ((event.modifiers & ~Qt.KeypadModifier) !== Qt.NoModifier) return;
        if (![Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key)) return;
        event.accepted = true;
        move(event.key);
    }
}
