#include <aribcaption/decoder.h>

// Give bindgen a typed constant for the cast-containing upstream macro.
static const int64_t ARIBCC_RS_DURATION_INDEFINITE = ARIBCC_DURATION_INDEFINITE;

uint32_t aribcc_rs_character(uint8_t set, uint8_t first, uint8_t second);
