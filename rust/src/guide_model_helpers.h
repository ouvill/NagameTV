#ifndef NAGAMETV_GUIDE_MODEL_HELPERS_H
#define NAGAMETV_GUIDE_MODEL_HELPERS_H
#include "channel_model_helpers.h"
#include <memory>
class GuideModel;
class GuideCandidates;
class GuideFilterModel;
template<typename T = GuideModel>
inline std::unique_ptr<T> makeGuideModel() { return std::make_unique<T>(); }
template<typename T = GuideCandidates>
inline std::unique_ptr<T> makeGuideCandidates() { return std::make_unique<T>(); }
template<typename T = GuideFilterModel>
inline std::unique_ptr<T> makeGuideFilterModel() { return std::make_unique<T>(); }
#endif
