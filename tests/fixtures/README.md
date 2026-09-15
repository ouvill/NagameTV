# Subtitle clock fixture

`subtitle-clock.ts` is a generated two-second, 64×64 black MPEG-2 video (25 fps),
with no audio or broadcast content. The subtitle clock regression test demuxes
it to memory and compares transport PTS to GStreamer video stream time.

Generate with GStreamer (CPU only, no display or audio devices):

```sh
gst-launch-1.0 -q videotestsrc num-buffers=50 pattern=black \
  ! video/x-raw,width=64,height=64,framerate=25/1 \
  ! avenc_mpeg2video ! mpegvideoparse ! mpegtsmux \
  ! filesink location=tests/fixtures/subtitle-clock.ts
```

## Recording playback fixture

`recording.ts` contains three seconds of synthetic 160×90 MPEG-2 video (25 fps)
and silent stereo AAC. It exercises local file playback in the production UI.
It contains no broadcast material. Generate using CPU-only sources and encoders:

```sh
gst-launch-1.0 -q mpegtsmux name=mux ! filesink location=tests/fixtures/recording.ts \
  videotestsrc num-buffers=75 pattern=ball \
    ! video/x-raw,width=160,height=90,framerate=25/1 \
    ! avenc_mpeg2video ! mpegvideoparse ! queue ! mux. \
  audiotestsrc num-buffers=141 wave=silence \
    ! audio/x-raw,rate=48000,channels=2 ! audioconvert ! avenc_aac ! aacparse ! queue ! mux.
```
