#ifndef NAGAMETV_CHANNEL_MODEL_HELPERS_H
#define NAGAMETV_CHANNEL_MODEL_HELPERS_H
#include <QtCore/QAbstractItemModel>
#include <QtCore/QSortFilterProxyModel>
#include <QtCore/QVariant>
#include <memory>
class ChannelModel;
class ChannelFilterModel;
template<typename T = ChannelModel>
inline std::unique_ptr<T> makeChannelModel() { return std::make_unique<T>(); }
// The model owns no domain state. These helpers adapt Qt's model API only.
template<typename T>
inline QVariant channelRow(const T &model, int row) {
    if (row < 0 || row >= model.rowCount(QModelIndex{})) return QVariantMap{};
    QVariantMap values;
    const auto roles = model.roleNames();
    const auto index = model.index(row, 0);
    for (auto it = roles.cbegin(); it != roles.cend(); ++it)
        values.insert(QString::fromUtf8(it.value()), model.data(index, it.key()));
    return values;
}
template<typename T>
inline int channelCount(const T &model) { return model.rowCount(QModelIndex{}); }
template<typename T>
inline QVariant channelSourceData(const T &model, int row, int role) {
    const auto source = model.sourceModel();
    return source ? source->data(source->index(row, 0), role) : QVariant{};
}
template<typename T>
inline int channelRowForIndex(const T &model, int wanted, int role) {
    for (int row = 0; row < model.rowCount(QModelIndex{}); ++row)
        if (model.data(model.index(row, 0), role).toInt() == wanted) return row;
    return -1;
}
template<typename T>
inline void connectChannelChanges(T &model) {
    const auto changed = [&model] { model.notifyChanged(); };
    QObject::connect(&model, &QAbstractItemModel::modelReset, &model, changed);
    QObject::connect(&model, &QAbstractItemModel::rowsInserted, &model, changed);
    QObject::connect(&model, &QAbstractItemModel::rowsRemoved, &model, changed);
    QObject::connect(&model, &QAbstractItemModel::layoutChanged, &model, changed);
    QObject::connect(&model, &QAbstractItemModel::dataChanged, &model, changed);
}
#endif
