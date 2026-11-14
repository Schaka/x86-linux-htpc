# Video

This page describes how each video format reaches the TV. [architecture.md](architecture.md) explains why some formats need a player outside of Plasma.

## Formats

| Format | What the TV gets | `JELLYFIN_MPV=system` | `JELLYFIN_MPV=cb1` and Kodi | Needs the patched kernel |
|---|---|---|---|---|
| SDR | SDR | mpv in Plasma | CB1 mpv or Kodi | No |
| HLG | HLG, or HDR10 after a conversion | mpv in Plasma | CB1 mpv or Kodi | No |
| HDR10 | HDR10. In Plasma, KWin tone-maps the video to the brightness range of the TV from the EDID. | mpv in Plasma | CB1 mpv or Kodi | No |
| HDR10+ | HDR10 with HDR10+ metadata for each frame | HDR10 without the metadata | CB1 mpv or Kodi | Yes |
| Dolby Vision | Dolby Vision, TV-led | HDR10 or tone-mapped, without the dynamic metadata | CB1 mpv or Kodi | Yes |

The mpv of Fedora 45 shows profile 5 files with wrong colors.

## Dolby Vision profiles

CB1 supports these Dolby Vision profiles:

| Profile | Typical source | Support |
|---|---|---|
| 5 | Streaming services | Yes |
| 7 with MEL (a small second layer that adds little) | UHD Blu-ray | Yes |
| 7 with FEL (a full second layer with more picture data) | UHD Blu-ray | Yes, with the second layer |
| 8.1, 8.2, 8.4 | Streaming, remuxes, HLG cameras | Yes |
| 10 | AV1 sources | Yes |

## HDR inside Plasma

For HDR on the TV, KWin needs HDR on for the output. `DISPLAY_HDR=yes` turns it on at each login. KWin keeps the output in HDR for all windows and does not switch it for each video. In HDR mode, KWin maps SDR windows into the HDR range, and they look pale on many TVs. So the default is `DISPLAY_HDR=no`.

The CB1 mpv on its own VT sets the signal for each video: SDR, HDR10 or Dolby Vision. It does not need this setting.

mpv tells KWin the color space and the brightness range of the video through the Wayland color management protocol (`target-colorspace-hint=yes`). mpv uses the output driver `gpu-next`.

KWin does not send the static metadata of the video to the TV. The HDR metadata on the connector (`HDR_OUTPUT_METADATA`) has the PQ curve and the values that KWin takes from the EDID. KWin gives mpv the brightness range of the TV, and mpv and KWin tone-map the video to this range. The TV gets a signal that already fits its range. Only a player that is DRM master on its own VT sends the metadata of the video itself.

## GPU load of mpv

At 4K, the GPU of an Intel N100 class machine has little time to spare in `gpu-next`:

| Case | Result |
|---|---|
| 4K HDR10 at 23.976 Hz, default options | The render passes take about 42 ms for each frame. A frame lasts 41.7 ms, so mpv drops about 3 frames each second. |
| 4K HDR10 at 23.976 Hz with `hdr-compute-peak=no` | No dropped frames |
| 4K at 60 Hz, SDR or HDR10, `gpu-next` | mpv drops about half of the frames, also with `profile=fast` |
| 4K at 60 Hz, SDR or HDR10, `vo=dmabuf-wayland` | No dropped frames. mpv gives the decoded frames to KWin without its own render pass. |

The project sets `hdr-compute-peak=no` for `jellyfin-mpv-shim`. For 4K videos at 50 and 60 Hz on an N100 class GPU, use the CB1 mpv. The mpv of the distribution drops about half of the frames there.

## Frame rate

Most films have 23.976 frames per second. On a 60 Hz output, every second frame shows for a different time, and motion judders. The output mode must match the frame rate of the video. `DISPLAY_MATCH_RATE=no` in the configuration file turns this off for `jellyfin-mpv-shim`.

1. Inside Plasma, no Wayland protocol lets a program change the display mode. The script `htpc-refresh.lua` in mpv changes the mode with `kscreen-doctor` before the first frame of each file. It picks the mode with the size of the current mode and the frame rate of the video, or two times the frame rate. A 23.976 fps video needs a 23.976 Hz mode, because 24 Hz shows one frame twice about every 42 seconds. Two seconds after mpv has no file, the script sets the old mode back.
2. The CB1 mpv on its own VT sets the display mode of each video in its DRM output (`htpc-output.lua`).
3. Kodi on its own VT changes the mode by itself.

## Hardware decode

mpv and Kodi decode the video on the GPU through VA-API. On Intel, the driver is `intel-media-driver`. Fedora ships a version of this driver without the patented codecs. The full version is in RPM Fusion. [distributions.md](distributions.md) has the details.
