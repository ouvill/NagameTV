#ifndef NAGAMETV_SUBTITLE_MODEL_HELPERS_H
#define NAGAMETV_SUBTITLE_MODEL_HELPERS_H
#include <memory>
class SubtitleModel;
template<typename T = SubtitleModel>
inline std::unique_ptr<T> makeSubtitleModel() { return std::make_unique<T>(); }
#endif
