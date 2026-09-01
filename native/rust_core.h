#pragma once

#include <cstdint>

extern "C" {
struct MirakurunPlayer;

MirakurunPlayer *mirakurun_player_create(char *error, std::uintptr_t error_capacity);
void mirakurun_player_destroy(MirakurunPlayer *player);
bool mirakurun_player_attach_video_item(MirakurunPlayer *player, void *item,
                                        char *error,
                                        std::uintptr_t error_capacity);
bool mirakurun_player_play_service(MirakurunPlayer *player, const char *server,
                                   std::uint64_t service_id, char *error,
                                   std::uintptr_t error_capacity);
void mirakurun_player_stop(MirakurunPlayer *player);
void mirakurun_player_set_pause(MirakurunPlayer *player, bool paused);
void mirakurun_player_set_volume(MirakurunPlayer *player, double volume);
int mirakurun_player_drain_events(MirakurunPlayer *player);
}
