#ifndef NAGAMETV_LAN_DISCOVERY_MODEL_HELPERS_H
#define NAGAMETV_LAN_DISCOVERY_MODEL_HELPERS_H
#include <memory>
class LanDiscoveryModel;
template<typename T = LanDiscoveryModel>
inline std::unique_ptr<T> makeLanDiscoveryModel() { return std::make_unique<T>(); }
#endif
