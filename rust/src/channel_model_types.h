#ifndef NAGAMETV_CHANNEL_MODEL_TYPES_H
#define NAGAMETV_CHANNEL_MODEL_TYPES_H
#include <QtCore/QSortFilterProxyModel>
#include <QtQml/qqmlregistration.h>
#include "cxx-qt-lib/qlist.h"
#include <type_traits>
// Refilter without resetting unchanged rows or destroying their QML delegates.
// Keep Qt 6.8 support while using the replacement API on Qt 6.10 and later.
class ChannelFilterProxy : public QSortFilterProxyModel {
    Q_OBJECT
    QML_ANONYMOUS
public:
    using QSortFilterProxyModel::QSortFilterProxyModel;
    void refreshFilter() {
#if QT_VERSION >= QT_VERSION_CHECK(6, 10, 0)
        beginFilterChange();
        endFilterChange(QSortFilterProxyModel::Direction::Rows);
#else
        invalidateRowsFilter();
#endif
    }
};
// Qt's proxy base must be visible to qmllint and the QML ahead-of-time compiler.
struct ChannelFilterBase {
    Q_GADGET
    QML_FOREIGN(QSortFilterProxyModel)
    QML_ANONYMOUS
};
// Resolve CXX's alias to QML's built-in list<int>, without creating another type.
struct ChannelIndexList {
    Q_GADGET
    QML_FOREIGN(QList_i32)
    // QML_USING token-pastes its argument, so template types need the underlying
    // class info directly. Check the alias in C++ as well as resolving it in moc.
    Q_CLASSINFO("QML.Using", "QList<int>")
};
static_assert(std::is_same_v<QList_i32, QList<int>>);
#endif
