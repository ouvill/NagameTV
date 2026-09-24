#pragma once
#include <ass/ass.h>
#include <cstdint>
#include <memory>
#include <stdexcept>
#include <vector>
#include "rust/cxx.h"

namespace media_captions {
// Qt-independent libass ownership. A renderer belongs to one subtitle stream;
// replacement destroys its track before the renderer and library.
class Renderer {
  ASS_Library *library = nullptr;
  ASS_Renderer *renderer = nullptr;
  ASS_Track *track = nullptr;
public:
  Renderer() {
    library = ass_library_init();
    if (!library) throw std::runtime_error("Could not initialize libass");
    renderer = ass_renderer_init(library);
    if (!renderer) { ass_library_done(library); throw std::runtime_error("Could not initialize subtitle renderer"); }
    ass_set_fonts(renderer, nullptr, "sans-serif", ASS_FONTPROVIDER_AUTODETECT, nullptr, 1);
    ass_set_cache_limits(renderer, 1000, 32);
    track = ass_new_track(library);
    if (!track) { ass_renderer_done(renderer); ass_library_done(library); throw std::runtime_error("Could not allocate subtitle track"); }
  }
  ~Renderer() { ass_free_track(track); ass_renderer_done(renderer); ass_library_done(library); }
  Renderer(const Renderer &) = delete;
  void header(rust::Slice<const std::uint8_t> bytes) {
    ass_process_codec_private(track, reinterpret_cast<char *>(const_cast<std::uint8_t *>(bytes.data())), bytes.size());
  }
  void chunk(rust::Slice<const std::uint8_t> bytes, std::int64_t start, std::int64_t duration) {
    ass_process_chunk(track, reinterpret_cast<char *>(const_cast<std::uint8_t *>(bytes.data())), bytes.size(), start, duration);
  }
  void script(rust::Slice<const std::uint8_t> bytes) {
    ASS_Track *loaded = ass_read_memory(library, reinterpret_cast<char *>(const_cast<std::uint8_t *>(bytes.data())), bytes.size(), nullptr);
    if (!loaded || loaded->n_events == 0) {
      if (loaded) ass_free_track(loaded);
      throw std::runtime_error("No subtitle events found");
    }
    ass_free_track(track);
    track = loaded;
  }
  void reset() {
    ASS_Track *next = ass_new_track(library);
    if (!next) throw std::runtime_error("Could not allocate subtitle track");
    ass_free_track(track); track = next;
  }
  rust::Vec<std::uint8_t> render(std::int64_t milliseconds, std::int32_t width, std::int32_t height, bool force) {
    if (width <= 0 || height <= 0 || width > 1920 || height > 1920)
      throw std::runtime_error("Invalid subtitle canvas");
    ass_set_frame_size(renderer, width, height);
    int changed = 0;
    ASS_Image *images = ass_render_frame(renderer, track, milliseconds, &changed);
    rust::Vec<std::uint8_t> pixels;
    if (!changed && !force) return pixels;
    pixels.reserve(static_cast<std::size_t>(width) * height * 4);
    for (std::size_t i = 0; i < static_cast<std::size_t>(width) * height * 4; ++i) pixels.push_back(0);
    for (ASS_Image *image = images; image; image = image->next) {
      const unsigned opacity = 255 - (image->color & 255);
      const unsigned rgb[] = { image->color >> 24, (image->color >> 16) & 255, (image->color >> 8) & 255 };
      for (int y = 0; y < image->h; ++y) {
        const int py = image->dst_y + y;
        if (py < 0 || py >= height) continue;
        for (int x = 0; x < image->w; ++x) {
          const int px = image->dst_x + x;
          if (px < 0 || px >= width) continue;
          const unsigned alpha = (image->bitmap[y * image->stride + x] * opacity + 127) / 255;
          const auto offset = (static_cast<std::size_t>(py) * width + px) * 4;
          for (int c = 0; c < 3; ++c)
            pixels[offset + c] = (rgb[c] * alpha + pixels[offset + c] * (255 - alpha) + 127) / 255;
          pixels[offset + 3] = alpha + (pixels[offset + 3] * (255 - alpha) + 127) / 255;
        }
      }
    }
    return pixels;
  }
};
inline std::unique_ptr<Renderer> makeRenderer() { return std::make_unique<Renderer>(); }
}
