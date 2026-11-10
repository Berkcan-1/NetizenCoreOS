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

## Resampling and wallpapers (new)

`resample.rs` replaces point sampling with a separable tent filter (area-aware when shrinking, bilinear when enlarging). `ref_resample.py` is an integer-exact Python mirror that matches Pillow's antialiased bilinear filter (mean error about 0.01 on normal ratios).

- `decode_png_preview` / `decode_jpeg_preview` now use that filter, so large photos no longer alias.
- `decode_png_wallpaper` / `decode_jpeg_wallpaper` / `compose_screen` fit an image to an exact screen size without stretching:
  - `FitMode::Auto`: aspect ratios within 12% use `Fill` (centered crop, scaled to cover); otherwise `Fit` (whole image, at most 4x enlargement, over a darkened blurred copy of itself).
  - `FitMode::Fill` and `FitMode::Fit` force one behavior.
- Peak memory is the decoded source (one row cache for filtering) plus the screen-sized result. Images above the existing 20 MP / 64 MiB budget, or that do not fit in the heap, return `ResourceLimit`.
