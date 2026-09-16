#include "filter.hpp"
#include <stdexcept>

namespace viewer {
namespace {
constexpr std::size_t transport_packet_bytes = 188;
constexpr std::uint8_t transport_sync_byte = 0x47;
constexpr int first_program_index = -1;
constexpr int complete_missing_stream = 1;
constexpr int insert_caption_management = 4;
}
Filter::Filter(std::uint16_t service) {
    // The Rust boundary reserves zero for automatic program selection.
    filter_.SetProgramNumberOrIndex(service == 0 ? first_program_index : service);
    filter_.SetCaptionMode(complete_missing_stream | insert_caption_management);
    filter_.SetSuperimposeMode(complete_missing_stream | insert_caption_management);
}
rust::Vec<std::uint8_t> Filter::push(rust::Slice<const std::uint8_t> packets) {
    if (packets.size() % transport_packet_bytes != 0) {
        throw std::invalid_argument("incomplete transport packet");
    }
    rust::Vec<std::uint8_t> output;
    for (std::size_t offset = 0; offset < packets.size(); offset += transport_packet_bytes) {
        if (packets[offset] != transport_sync_byte) {
            throw std::invalid_argument("missing transport sync byte");
        }
        filter_.AddPacket(packets.data() + offset);
        for (auto byte : filter_.GetPackets()) output.push_back(byte);
        filter_.ClearPackets();
    }
    return output;
}
std::unique_ptr<Filter> make_filter(std::uint16_t service) {
    return std::make_unique<Filter>(service);
}
}
