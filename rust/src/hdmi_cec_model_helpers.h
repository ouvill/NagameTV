#ifndef NAGAMETV_HDMI_CEC_MODEL_HELPERS_H
#define NAGAMETV_HDMI_CEC_MODEL_HELPERS_H
#include <memory>
class HdmiCecDeviceModel;
template<typename T = HdmiCecDeviceModel>
inline std::unique_ptr<T> makeHdmiCecDeviceModel() { return std::make_unique<T>(); }
#endif
