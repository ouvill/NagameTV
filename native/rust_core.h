#pragma once

#include <cstdint>

extern "C" {
struct MirakurunPlayer;
using GlGetProcAddress = void *(*)(void *context, const char *name);

MirakurunPlayer *mirakurun_player_create(char *error, std::uintptr_t error_capacity);
void mirakurun_player_destroy(MirakurunPlayer *player);
bool mirakurun_player_play_service(MirakurunPlayer *player, const char *server,
                                   std::uint64_t service_id, char *error,
                                   std::uintptr_t error_capacity);
void mirakurun_player_stop(MirakurunPlayer *player);
void mirakurun_player_set_pause(MirakurunPlayer *player, bool paused);
void mirakurun_player_set_volume(MirakurunPlayer *player, double volume);
int mirakurun_player_drain_events(MirakurunPlayer *player);
bool mirakurun_player_init_renderer(MirakurunPlayer *player,
                                    GlGetProcAddress get_proc, void *context,
                                    char *error, std::uintptr_t error_capacity);
bool mirakurun_player_render(MirakurunPlayer *player, int framebuffer,
                             int width, int height);
void mirakurun_player_free_renderer(MirakurunPlayer *player);
}
