#ifndef NAGAMETV_CHANNEL_MODEL_TEST_H
#define NAGAMETV_CHANNEL_MODEL_TEST_H
#include <QtTest/QAbstractItemModelTester>
#include <QtTest/QSignalSpy>
#include <QtCore/QPersistentModelIndex>
#include <memory>
class ChannelModelSignals {
    QSignalSpy resets_;
    QSignalSpy updates_;
    QPersistentModelIndex persistent_;
public:
    explicit ChannelModelSignals(QAbstractItemModel &model)
        : resets_(&model, &QAbstractItemModel::modelReset),
          updates_(&model, &QAbstractItemModel::dataChanged),
          persistent_(model.index(0, 0)) {}
    int resets() const { return resets_.count(); }
    int updates() const { return updates_.count(); }
    int updated_row(int event) const { return updates_.at(event).at(0).value<QModelIndex>().row(); }
    bool favorite_roles(int event, int favorite, int order) const {
        const auto &args = updates_.at(event);
        return args.at(0).value<QModelIndex>() == args.at(1).value<QModelIndex>()
            && args.at(2).value<QList<int>>() == QList<int>{favorite, order};
    }
    int persistent_row() const { return persistent_.isValid() ? persistent_.row() : -1; }
};
template<typename T>
inline std::unique_ptr<ChannelModelSignals> observeChannelModel(T &model) {
    return std::make_unique<ChannelModelSignals>(model);
}
class ChannelFilterModel;
template<typename T = ChannelFilterModel>
inline std::unique_ptr<T> makeChannelFilterModel() { return std::make_unique<T>(); }
template<typename T, typename S>
inline void setChannelTestSource(T &filter, S &source) { filter.setSourceModel(&source); }
template<typename T>
inline void checkChannelModel(T &model) {
    new QAbstractItemModelTester(&model, QAbstractItemModelTester::FailureReportingMode::Fatal, &model);
}
#endif
