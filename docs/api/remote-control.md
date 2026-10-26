# Protocol Documentation
<a name="top"></a>

## Table of Contents

- [viewer/v1/player.proto](#viewer_v1_player-proto)
    - [ActivePlayback](#viewer-v1-ActivePlayback)
    - [Channel](#viewer-v1-Channel)
    - [FilePlayback](#viewer-v1-FilePlayback)
    - [GetStateRequest](#viewer-v1-GetStateRequest)
    - [GetStateResponse](#viewer-v1-GetStateResponse)
    - [ListChannelsRequest](#viewer-v1-ListChannelsRequest)
    - [ListChannelsResponse](#viewer-v1-ListChannelsResponse)
    - [PlayRequest](#viewer-v1-PlayRequest)
    - [PlayResponse](#viewer-v1-PlayResponse)
    - [PlayerState](#viewer-v1-PlayerState)
    - [PreviousChannelRequest](#viewer-v1-PreviousChannelRequest)
    - [PreviousChannelResponse](#viewer-v1-PreviousChannelResponse)
    - [Program](#viewer-v1-Program)
    - [ReturnToLiveRequest](#viewer-v1-ReturnToLiveRequest)
    - [ReturnToLiveResponse](#viewer-v1-ReturnToLiveResponse)
    - [SeekRelativeRequest](#viewer-v1-SeekRelativeRequest)
    - [SeekRelativeResponse](#viewer-v1-SeekRelativeResponse)
    - [SeekToRequest](#viewer-v1-SeekToRequest)
    - [SeekToResponse](#viewer-v1-SeekToResponse)
    - [SelectChannelRequest](#viewer-v1-SelectChannelRequest)
    - [SelectChannelResponse](#viewer-v1-SelectChannelResponse)
    - [SelectRemoteKeyRequest](#viewer-v1-SelectRemoteKeyRequest)
    - [SelectRemoteKeyResponse](#viewer-v1-SelectRemoteKeyResponse)
    - [SendKeyRequest](#viewer-v1-SendKeyRequest)
    - [SendKeyResponse](#viewer-v1-SendKeyResponse)
    - [SetMutedRequest](#viewer-v1-SetMutedRequest)
    - [SetMutedResponse](#viewer-v1-SetMutedResponse)
    - [SetPausedRequest](#viewer-v1-SetPausedRequest)
    - [SetPausedResponse](#viewer-v1-SetPausedResponse)
    - [SetPlaybackRateRequest](#viewer-v1-SetPlaybackRateRequest)
    - [SetPlaybackRateResponse](#viewer-v1-SetPlaybackRateResponse)
    - [SetSubtitlesRequest](#viewer-v1-SetSubtitlesRequest)
    - [SetSubtitlesResponse](#viewer-v1-SetSubtitlesResponse)
    - [SetVolumeRequest](#viewer-v1-SetVolumeRequest)
    - [SetVolumeResponse](#viewer-v1-SetVolumeResponse)
    - [StepChannelRequest](#viewer-v1-StepChannelRequest)
    - [StepChannelResponse](#viewer-v1-StepChannelResponse)
    - [StopRequest](#viewer-v1-StopRequest)
    - [StopResponse](#viewer-v1-StopResponse)
    - [StoppedPlayback](#viewer-v1-StoppedPlayback)
    - [WatchStateRequest](#viewer-v1-WatchStateRequest)
    - [WatchStateResponse](#viewer-v1-WatchStateResponse)

    - [BroadcastBand](#viewer-v1-BroadcastBand)
    - [RemoteCommandKind](#viewer-v1-RemoteCommandKind)
    - [RemoteKey](#viewer-v1-RemoteKey)
    - [SubtitleDisplay](#viewer-v1-SubtitleDisplay)

    - [PlayerService](#viewer-v1-PlayerService)

- [Scalar Value Types](#scalar-value-types)



<a name="viewer_v1_player-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## viewer/v1/player.proto



<a name="viewer-v1-ActivePlayback"></a>

### ActivePlayback



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| channel_id | [uint64](#uint64) |  | Mirakurun catalog ID of the current stream attempt, retained on stop failure. |






<a name="viewer-v1-Channel"></a>

### Channel



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [uint64](#uint64) |  |  |
| name | [string](#string) |  |  |
| label | [string](#string) |  |  |
| band | [BroadcastBand](#viewer-v1-BroadcastBand) |  |  |
| current_program | [Program](#viewer-v1-Program) |  | Current wall-clock EPG program; absent if unavailable or ambiguous. |
| remote_key | [uint32](#uint32) | optional | Desktop remote-control number, absent when unassigned/outside 1 through 12. |






<a name="viewer-v1-FilePlayback"></a>

### FilePlayback



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| name | [string](#string) |  | Display filename only; the local filesystem path is not published. |






<a name="viewer-v1-GetStateRequest"></a>

### GetStateRequest







<a name="viewer-v1-GetStateResponse"></a>

### GetStateResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| state | [PlayerState](#viewer-v1-PlayerState) |  | Always present in a successful response. |






<a name="viewer-v1-ListChannelsRequest"></a>

### ListChannelsRequest







<a name="viewer-v1-ListChannelsResponse"></a>

### ListChannelsResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| channels | [Channel](#viewer-v1-Channel) | repeated | Opaque IDs belong to the currently configured Mirakurun server. |






<a name="viewer-v1-PlayRequest"></a>

### PlayRequest







<a name="viewer-v1-PlayResponse"></a>

### PlayResponse







<a name="viewer-v1-PlayerState"></a>

### PlayerState
A consistent projection of the player&#39;s authoritative state.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| revision | [uint64](#uint64) |  | Increases on published state changes; resets when the API restarts. |
| selected_channel_id | [uint64](#uint64) | optional | May differ from the active channel after catalog updates or failed switches. |
| stopped | [StoppedPlayback](#viewer-v1-StoppedPlayback) |  |  |
| connecting | [ActivePlayback](#viewer-v1-ActivePlayback) |  |  |
| playing | [ActivePlayback](#viewer-v1-ActivePlayback) |  |  |
| stop_failed | [ActivePlayback](#viewer-v1-ActivePlayback) |  |  |
| file_connecting | [FilePlayback](#viewer-v1-FilePlayback) |  |  |
| file_playing | [FilePlayback](#viewer-v1-FilePlayback) |  |  |
| file_stop_failed | [FilePlayback](#viewer-v1-FilePlayback) |  |  |
| file_paused | [FilePlayback](#viewer-v1-FilePlayback) |  |  |
| file_seeking | [FilePlayback](#viewer-v1-FilePlayback) |  |  |
| file_seeking_paused | [FilePlayback](#viewer-v1-FilePlayback) |  |  |
| file_ended | [FilePlayback](#viewer-v1-FilePlayback) |  |  |
| paused | [ActivePlayback](#viewer-v1-ActivePlayback) |  |  |
| seeking | [ActivePlayback](#viewer-v1-ActivePlayback) |  |  |
| seeking_paused | [ActivePlayback](#viewer-v1-ActivePlayback) |  |  |
| ended | [ActivePlayback](#viewer-v1-ActivePlayback) |  |  |
| volume_fraction | [double](#double) |  | Selected volume, even while muted, in the inclusive range 0.0 to 1.0. |
| muted | [bool](#bool) |  |  |
| subtitles | [SubtitleDisplay](#viewer-v1-SubtitleDisplay) |  |  |
| playback_error | [string](#string) |  | Last playback error, empty if none. Human-readable; not a stable error code. |
| current_program | [Program](#viewer-v1-Program) |  | Current program for the active channel, absent when unknown or stopped. |
| settings_error | [string](#string) |  | Last settings write error; a successful control RPC does not guarantee persistence. |
| position_ms | [uint64](#uint64) | optional | Absent until known. Live values use the retained session timeline, not Unix time. |
| duration_ms | [uint64](#uint64) | optional | Recording duration, or the end of the live session timeline; absent if unknown. |
| seek_start_ms | [uint64](#uint64) | optional | Both present together when seeking is available; endpoints are inclusive. |
| seek_end_ms | [uint64](#uint64) | optional |  |
| playback_rate_tenths | [int32](#int32) |  | Applied speed; 10 means normal speed, even while inactive. |
| can_pause | [bool](#bool) |  |  |
| can_seek | [bool](#bool) |  |  |
| can_return_to_live | [bool](#bool) |  |  |
| can_set_rate | [bool](#bool) |  |  |
| at_live_edge | [bool](#bool) |  | True only while actively playing at the live edge (not paused near the edge). |
| can_step_channel | [bool](#bool) |  |  |
| can_previous_channel | [bool](#bool) |  |  |






<a name="viewer-v1-PreviousChannelRequest"></a>

### PreviousChannelRequest







<a name="viewer-v1-PreviousChannelResponse"></a>

### PreviousChannelResponse







<a name="viewer-v1-Program"></a>

### Program



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [uint64](#uint64) |  |  |
| name | [string](#string) |  |  |
| description | [string](#string) |  |  |
| start_at_ms | [uint64](#uint64) |  | Unix epoch milliseconds. |
| duration_ms | [uint64](#uint64) |  |  |






<a name="viewer-v1-ReturnToLiveRequest"></a>

### ReturnToLiveRequest







<a name="viewer-v1-ReturnToLiveResponse"></a>

### ReturnToLiveResponse







<a name="viewer-v1-SeekRelativeRequest"></a>

### SeekRelativeRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| delta_ms | [sint64](#sint64) | optional | Required. Inclusive range -9007199254740991 to 9007199254740991. |






<a name="viewer-v1-SeekRelativeResponse"></a>

### SeekRelativeResponse







<a name="viewer-v1-SeekToRequest"></a>

### SeekToRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| position_ms | [uint64](#uint64) | optional | Required. Inclusive range 0 to 9007199254740991. |






<a name="viewer-v1-SeekToResponse"></a>

### SeekToResponse







<a name="viewer-v1-SelectChannelRequest"></a>

### SelectChannelRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| channel_id | [uint64](#uint64) | optional | Required. Mirakurun&#39;s opaque channel ID, not the catalog index or broadcast serviceId. |






<a name="viewer-v1-SelectChannelResponse"></a>

### SelectChannelResponse







<a name="viewer-v1-SelectRemoteKeyRequest"></a>

### SelectRemoteKeyRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| number | [uint32](#uint32) | optional | Required, 1 through 12 inclusive. Missing catalog match returns NOT_FOUND. |
| band | [BroadcastBand](#viewer-v1-BroadcastBand) |  | UNSPECIFIED uses the viewed/selected band, or terrestrial with no selection. |






<a name="viewer-v1-SelectRemoteKeyResponse"></a>

### SelectRemoteKeyResponse







<a name="viewer-v1-SendKeyRequest"></a>

### SendKeyRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| key | [RemoteKey](#viewer-v1-RemoteKey) |  |  |






<a name="viewer-v1-SendKeyResponse"></a>

### SendKeyResponse







<a name="viewer-v1-SetMutedRequest"></a>

### SetMutedRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| muted | [bool](#bool) | optional | Required, including when setting false. |






<a name="viewer-v1-SetMutedResponse"></a>

### SetMutedResponse







<a name="viewer-v1-SetPausedRequest"></a>

### SetPausedRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| paused | [bool](#bool) | optional | Required, including false. |






<a name="viewer-v1-SetPausedResponse"></a>

### SetPausedResponse







<a name="viewer-v1-SetPlaybackRateRequest"></a>

### SetPlaybackRateRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| tenths | [int32](#int32) | optional | Required. 5 through 20 inclusive: 0.5x through 2.0x, in steps of 0.1x. |






<a name="viewer-v1-SetPlaybackRateResponse"></a>

### SetPlaybackRateResponse







<a name="viewer-v1-SetSubtitlesRequest"></a>

### SetSubtitlesRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| visible | [bool](#bool) | optional | Required, including when setting false. |






<a name="viewer-v1-SetSubtitlesResponse"></a>

### SetSubtitlesResponse







<a name="viewer-v1-SetVolumeRequest"></a>

### SetVolumeRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| fraction | [double](#double) | optional | Required, finite, inclusive range 0.0 to 1.0. Out-of-range values are rejected. |






<a name="viewer-v1-SetVolumeResponse"></a>

### SetVolumeResponse







<a name="viewer-v1-StepChannelRequest"></a>

### StepChannelRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| offset | [int32](#int32) | optional | Required. Exactly -1 (previous) or &#43;1 (next). |






<a name="viewer-v1-StepChannelResponse"></a>

### StepChannelResponse







<a name="viewer-v1-StopRequest"></a>

### StopRequest







<a name="viewer-v1-StopResponse"></a>

### StopResponse







<a name="viewer-v1-StoppedPlayback"></a>

### StoppedPlayback







<a name="viewer-v1-WatchStateRequest"></a>

### WatchStateRequest







<a name="viewer-v1-WatchStateResponse"></a>

### WatchStateResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| state | [PlayerState](#viewer-v1-PlayerState) |  | Always present; never a delta. |








<a name="viewer-v1-BroadcastBand"></a>

### BroadcastBand


| Name | Number | Description |
| ---- | ------ | ----------- |
| BROADCAST_BAND_UNSPECIFIED | 0 |  |
| BROADCAST_BAND_TERRESTRIAL | 1 |  |
| BROADCAST_BAND_BS | 2 |  |
| BROADCAST_BAND_CS | 3 |  |
| BROADCAST_BAND_SKY | 4 |  |
| BROADCAST_BAND_OTHER | 5 |  |



<a name="viewer-v1-RemoteCommandKind"></a>

### RemoteCommandKind
Values emitted by Player.remote_command_executed(kind) after successful execution.

| Name | Number | Description |
| ---- | ------ | ----------- |
| REMOTE_COMMAND_KIND_UNSPECIFIED | 0 |  |
| REMOTE_COMMAND_KIND_SELECT_CHANNEL | 1 |  |
| REMOTE_COMMAND_KIND_PLAY | 2 |  |
| REMOTE_COMMAND_KIND_STOP | 3 |  |
| REMOTE_COMMAND_KIND_SET_VOLUME | 4 |  |
| REMOTE_COMMAND_KIND_SET_MUTED | 5 |  |
| REMOTE_COMMAND_KIND_SET_SUBTITLES | 6 |  |
| REMOTE_COMMAND_KIND_SET_PAUSED | 7 |  |
| REMOTE_COMMAND_KIND_SEEK_RELATIVE | 8 |  |
| REMOTE_COMMAND_KIND_SEEK_TO | 9 |  |
| REMOTE_COMMAND_KIND_RETURN_TO_LIVE | 10 |  |
| REMOTE_COMMAND_KIND_SET_PLAYBACK_RATE | 11 |  |
| REMOTE_COMMAND_KIND_STEP_CHANNEL | 12 |  |
| REMOTE_COMMAND_KIND_PREVIOUS_CHANNEL | 13 |  |
| REMOTE_COMMAND_KIND_SELECT_REMOTE_KEY | 14 |  |
| REMOTE_COMMAND_KIND_SEND_KEY | 15 |  |



<a name="viewer-v1-RemoteKey"></a>

### RemoteKey


| Name | Number | Description |
| ---- | ------ | ----------- |
| REMOTE_KEY_UNSPECIFIED | 0 |  |
| REMOTE_KEY_UP | 1 |  |
| REMOTE_KEY_DOWN | 2 |  |
| REMOTE_KEY_LEFT | 3 |  |
| REMOTE_KEY_RIGHT | 4 |  |
| REMOTE_KEY_OK | 5 |  |
| REMOTE_KEY_BACK | 6 |  |
| REMOTE_KEY_GUIDE | 7 |  |
| REMOTE_KEY_CHANNELS | 8 |  |
| REMOTE_KEY_FULLSCREEN | 9 |  |



<a name="viewer-v1-SubtitleDisplay"></a>

### SubtitleDisplay


| Name | Number | Description |
| ---- | ------ | ----------- |
| SUBTITLE_DISPLAY_UNSPECIFIED | 0 |  |
| SUBTITLE_DISPLAY_DISABLED | 1 |  |
| SUBTITLE_DISPLAY_HIDDEN | 2 |  |
| SUBTITLE_DISPLAY_VISIBLE | 3 |  |







<a name="viewer-v1-PlayerService"></a>

### PlayerService
Controls one running desktop viewer on a trusted home LAN. All RPCs are available
without authentication to clients that can reach the configured listener.
Commands run in order on the player&#39;s Qt thread. Success acknowledges command
execution, not receipt of video or durable settings storage. Observe WatchState
for asynchronous playback failures. Timed-out commands may already have run;
reconcile using GetState before retrying. No command is automatically replayed.

| Method Name | Request Type | Response Type | Description |
| ----------- | ------------ | ------------- | ------------|
| ListChannels | [ListChannelsRequest](#viewer-v1-ListChannelsRequest) | [ListChannelsResponse](#viewer-v1-ListChannelsResponse) | Returns the current catalog in the viewer&#39;s channel order, including subchannels. |
| GetState | [GetStateRequest](#viewer-v1-GetStateRequest) | [GetStateResponse](#viewer-v1-GetStateResponse) | Returns the most recently published state (normally within one 50 ms UI tick). |
| WatchState | [WatchStateRequest](#viewer-v1-WatchStateRequest) | [WatchStateResponse](#viewer-v1-WatchStateResponse) stream | Sends a full state immediately, then the latest state on change. Slow readers may skip intermediate states. Reconnect explicitly; every connection starts with a fresh full state. Revisions are scoped to this server lifetime. |
| SelectChannel | [SelectChannelRequest](#viewer-v1-SelectChannelRequest) | [SelectChannelResponse](#viewer-v1-SelectChannelResponse) | Selects a catalog ID and starts playback. Unknown IDs return NOT_FOUND. |
| Play | [PlayRequest](#viewer-v1-PlayRequest) | [PlayResponse](#viewer-v1-PlayResponse) | Starts the selected channel or replays the locally selected recording. Resumes a paused recording; restarts an ended recording from the beginning. No selection/output returns FAILED_PRECONDITION. Does not open arbitrary files. |
| Stop | [StopRequest](#viewer-v1-StopRequest) | [StopResponse](#viewer-v1-StopResponse) | Stops playback and cancels pending startup autoplay. Safe to repeat. |
| SetVolume | [SetVolumeRequest](#viewer-v1-SetVolumeRequest) | [SetVolumeResponse](#viewer-v1-SetVolumeResponse) | Sets the volume fraction and unmutes, matching the desktop volume slider. |
| SetMuted | [SetMutedRequest](#viewer-v1-SetMutedRequest) | [SetMutedResponse](#viewer-v1-SetMutedResponse) | Sets mute explicitly. Unmuting restores the selected volume. |
| SetSubtitles | [SetSubtitlesRequest](#viewer-v1-SetSubtitlesRequest) | [SetSubtitlesResponse](#viewer-v1-SetSubtitlesResponse) | Controls subtitle display. A disabled subtitle feature returns FAILED_PRECONDITION. |
| SetPaused | [SetPausedRequest](#viewer-v1-SetPausedRequest) | [SetPausedResponse](#viewer-v1-SetPausedResponse) | Explicit pause/resume. Requires can_pause; safe to repeat. |
| SeekRelative | [SeekRelativeRequest](#viewer-v1-SeekRelativeRequest) | [SeekRelativeResponse](#viewer-v1-SeekRelativeResponse) | Relative to the current position; clamps to the current seekable range. |
| SeekTo | [SeekToRequest](#viewer-v1-SeekToRequest) | [SeekToResponse](#viewer-v1-SeekToResponse) | Uses the same millisecond timeline as PlayerState; clamps to its seekable range. |
| ReturnToLive | [ReturnToLiveRequest](#viewer-v1-ReturnToLiveRequest) | [ReturnToLiveResponse](#viewer-v1-ReturnToLiveResponse) | Returns retained live playback to the live edge and normal speed. |
| SetPlaybackRate | [SetPlaybackRateRequest](#viewer-v1-SetPlaybackRateRequest) | [SetPlaybackRateResponse](#viewer-v1-SetPlaybackRateResponse) | Changes speed in tenths, matching the desktop controls. Requires can_set_rate. |
| StepChannel | [StepChannelRequest](#viewer-v1-StepChannelRequest) | [StepChannelResponse](#viewer-v1-StepChannelResponse) | Uses the desktop channel order and program-aware subchannel skipping. |
| PreviousChannel | [PreviousChannelRequest](#viewer-v1-PreviousChannelRequest) | [PreviousChannelResponse](#viewer-v1-PreviousChannelResponse) | Selects the last viewed channel using desktop viewing history. |
| SelectRemoteKey | [SelectRemoteKeyRequest](#viewer-v1-SelectRemoteKeyRequest) | [SelectRemoteKeyResponse](#viewer-v1-SelectRemoteKeyResponse) | Selects a remote-control number, using the viewed/selected band when unspecified. |
| SendKey | [SendKeyRequest](#viewer-v1-SendKeyRequest) | [SendKeyResponse](#viewer-v1-SendKeyResponse) | Success means the validated key was emitted to QML on the Qt thread, not that QML completed an action. Unknown and unspecified keys are rejected. |





## Scalar Value Types

| .proto Type | Notes | C++ | Java | Python | Go | C# | PHP | Ruby |
| ----------- | ----- | --- | ---- | ------ | -- | -- | --- | ---- |
| <a name="double" /> double |  | double | double | float | float64 | double | float | Float |
| <a name="float" /> float |  | float | float | float | float32 | float | float | Float |
| <a name="int32" /> int32 | Uses variable-length encoding. Inefficient for encoding negative numbers – if your field is likely to have negative values, use sint32 instead. | int32 | int | int | int32 | int | integer | Bignum or Fixnum (as required) |
| <a name="int64" /> int64 | Uses variable-length encoding. Inefficient for encoding negative numbers – if your field is likely to have negative values, use sint64 instead. | int64 | long | int/long | int64 | long | integer/string | Bignum |
| <a name="uint32" /> uint32 | Uses variable-length encoding. | uint32 | int | int/long | uint32 | uint | integer | Bignum or Fixnum (as required) |
| <a name="uint64" /> uint64 | Uses variable-length encoding. | uint64 | long | int/long | uint64 | ulong | integer/string | Bignum or Fixnum (as required) |
| <a name="sint32" /> sint32 | Uses variable-length encoding. Signed int value. These more efficiently encode negative numbers than regular int32s. | int32 | int | int | int32 | int | integer | Bignum or Fixnum (as required) |
| <a name="sint64" /> sint64 | Uses variable-length encoding. Signed int value. These more efficiently encode negative numbers than regular int64s. | int64 | long | int/long | int64 | long | integer/string | Bignum |
| <a name="fixed32" /> fixed32 | Always four bytes. More efficient than uint32 if values are often greater than 2^28. | uint32 | int | int | uint32 | uint | integer | Bignum or Fixnum (as required) |
| <a name="fixed64" /> fixed64 | Always eight bytes. More efficient than uint64 if values are often greater than 2^56. | uint64 | long | int/long | uint64 | ulong | integer/string | Bignum |
| <a name="sfixed32" /> sfixed32 | Always four bytes. | int32 | int | int | int32 | int | integer | Bignum or Fixnum (as required) |
| <a name="sfixed64" /> sfixed64 | Always eight bytes. | int64 | long | int/long | int64 | long | integer/string | Bignum |
| <a name="bool" /> bool |  | bool | boolean | boolean | bool | bool | boolean | TrueClass/FalseClass |
| <a name="string" /> string | A string must always contain UTF-8 encoded or 7-bit ASCII text. | string | String | str/unicode | string | string | string | String (UTF-8) |
| <a name="bytes" /> bytes | May contain any arbitrary sequence of bytes. | string | ByteString | str | []byte | ByteString | string | String (ASCII-8BIT) |
