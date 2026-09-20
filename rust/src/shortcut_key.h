#ifndef NAGAMETV_SHORTCUT_KEY_H
#define NAGAMETV_SHORTCUT_KEY_H
#include <QtGui/QKeySequence>

// Use Qt's portable sequence parser for the same single-combination bindings
// registered by QML Shortcut. No platform scan-code or keyboard-layout table.
inline bool matchesShortcutKey(const QString &sequence, int key, int modifiers) {
    const QKeySequence binding(sequence, QKeySequence::PortableText);
    const auto combination = QKeyCombination(Qt::KeyboardModifiers(modifiers) & ~Qt::KeypadModifier,
                                            Qt::Key(key));
    return binding.count() == 1 && binding[0] == combination;
}
#endif
