// Narrow, stateless lookup into the pinned libaribcaption tables. Protocol state
// and bounds are owned by Rust; checks here also protect direct FFI callers.
#include "decoder/b24_conv_tables.hpp"
#include "decoder/b24_gaiji_table.hpp"
extern "C" uint32_t aribcc_rs_character(uint8_t set, uint8_t first, uint8_t second) {
    using namespace aribcaption;
    if (first < 0x21 || first > 0x7e) return 0xfffd;
    const unsigned index = first - 0x21;
    switch (set) {
    case 1: return kAlphanumericTable_Halfwidth[index];
    case 2: return kHiraganaTable[index];
    case 3: return kKatakanaTable[index];
    case 4: return kJISX0201KatakanaTable[index];
    case 0:
        if (second < 0x21 || second > 0x7e) return 0xfffd;
        return index < 84 ? kKanjiTable[index * 94 + second - 0x21]
            : kAdditionalSymbolsTable_Unicode[(index - 84) * 94 + second - 0x21];
    default: return 0xfffd;
    }
}
