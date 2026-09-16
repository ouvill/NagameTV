#pragma once
#include "rust/cxx.h"
#include "servicefilter.hpp"
#include <memory>

namespace viewer {
class Filter {
public:
    explicit Filter(std::uint16_t service);
    rust::Vec<std::uint8_t> push(rust::Slice<const std::uint8_t> packets);
private:
    CServiceFilter filter_;
};
std::unique_ptr<Filter> make_filter(std::uint16_t service);
}
