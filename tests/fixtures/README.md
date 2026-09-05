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
