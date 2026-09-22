#ifndef NAGAMETV_CHANNEL_MODEL_TEST_H
#define NAGAMETV_CHANNEL_MODEL_TEST_H
#include <QtTest/QAbstractItemModelTester>
#include <memory>
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
