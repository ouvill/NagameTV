#ifndef NAGAMETV_VIDEO_FILE_MODEL_HELPERS_H
#define NAGAMETV_VIDEO_FILE_MODEL_HELPERS_H
#include <memory>
class VideoFileModel;
template<typename T = VideoFileModel>
inline std::unique_ptr<T> makeVideoFileModel() { return std::make_unique<T>(); }
#endif
