#ifndef NAGAMETV_RECORDING_MODEL_HELPERS_H
#define NAGAMETV_RECORDING_MODEL_HELPERS_H
#include <memory>
class RecordingModel;
template<typename T = RecordingModel>
inline std::unique_ptr<T> makeRecordingModel() { return std::make_unique<T>(); }
#endif
