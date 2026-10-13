#ifndef NAGAMETV_CONTINUE_WATCHING_MODEL_HELPERS_H
#define NAGAMETV_CONTINUE_WATCHING_MODEL_HELPERS_H
#include <memory>
class ContinueWatchingModel;
template<typename T = ContinueWatchingModel>
inline std::unique_ptr<T> makeContinueWatchingModel() { return std::make_unique<T>(); }
#endif
