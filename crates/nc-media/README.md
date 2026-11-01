# nc-media

`nc-media` is a `no_std` media pipeline for NetizenCore. It validates signatures and container bounds, decodes PNG/JPEG into bounded ARGB rasters, and streams MP3/WAV PCM16 to the hardware-independent `nc-hal::AudioOutput` contract.

## Supported formats

| Format | Support | Limits |
|---|---|---|
| BMP | 24/32-bit uncompressed pixel decode in `nc-app-files` | BI_RGB only |
| PNG | Real decode via `nopng`, native pixel formats sampled into bounded ARGB preview | 32 MiB encoded; 20 MP / 64 MiB source budget; 1280×800 preview cap; APNG rejected |
| JPEG | Real RGB decode via `zune-jpeg` safe scalar path | 32 MiB encoded; 20 MP / 64 MiB source budget; 1280×800 preview cap |
| MP3 | Layer III decode using `mbop3`; emits PCM16 | Streaming fixed-size buffers; requires an audio backend |
| WAV | RIFF/WAVE PCM16 mono/stereo streaming | Other formats are rejected |
| MP4 | Container/track metadata only | No sample demux, video/audio decode, or presentation sync |

## Streaming

`MediaSource` provides bounded random-access reads. `play_mp3()` and `play_wav_pcm16()` decode from that source without copying a complete audio file into memory. The file manager adds a VFS-backed bridge; playback is blocking and must run on a separate audio worker.

PNG/JPEG decode still performs one synchronous codec call once the bounded file-read phase has completed. The drawing path uses `Canvas::draw_image_bilinear()` to blit the preview directly into the framebuffer; fully preemptible codec work still needs the OS task executor to be wired into `nc-app-files`.

MP4 playback is intentionally not claimed. It still needs MP4 sample demuxing, a video codec such as H.264, an audio codec such as AAC, a frame presenter, and A/V synchronization.
