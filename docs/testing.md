# Testing

`sudo ./install.sh --check` runs the checks of all components. A check reads the state of the system, for example whether a service runs or whether the HDMI sink offers the audio formats. Some results show only on the TV or the receiver, for example the Dolby Vision mode. This page shows how to see each result.

## Each format

| Format | On the HTPC | On the TV or the receiver |
|---|---|---|
| Dolby Vision | The connector state shows the Dolby Vision vendor InfoFrame, in `/sys/kernel/debug/dri/<card>/i915_display_info` on Intel. The log of the CB1 mpv or of Kodi shows the CB1 output mode. | The TV shows its Dolby Vision logo or mode |
| HDR10 | The connector has an `HDR_OUTPUT_METADATA` blob with the PQ transfer function. `drm_info` shows it. | The TV shows HDR |
| HDR10+ | The connector has an `HDR10PLUS_METADATA` blob that changes during playback | The TV shows HDR10+ |
| Frame rate | The output mode matches the frame rate of the video | Motion in a slow camera pan is smooth |
| Passthrough | `/proc/asound/card<n>/pcm<m>p/sub0/hw_params` shows the format `IEC958_SUBFRAME_LE`. For TrueHD and DTS-HD Master Audio, it also shows 8 channels at 192000 Hz. | The receiver shows the format, for example "Dolby Atmos" or "DTS:X" |
| Android apps | `waydroid status` shows a running session, and the app starts | A 4K video plays without dropped frames |

## Casting

Two test sets cover the casting from phones and computers.

| Test | What it checks | How to run it |
|---|---|---|
| Rust tests of `htpc-control` | The DLNA, REST and DIAL requests, the choice between audio and video playback, the cold start of VacuumTube through DIAL, and the identity that the phone sees. The tests use a fake VacuumTube and a fake start of mpv, so they start no program. | `cd control && cargo test` |
| `tools/test-casting.py HOST` | A running HTPC, from another computer on the network, as a phone does: the DLNA search, music with pause, seek and stop, the REST API, the DIAL search and the app resource, and the AirPlay receiver | `tools/test-casting.py HOST --ssh USER@HOST` |

The script plays a silent file, and it needs `ffmpeg`. The options `--video` and `--launch` also cast a test video and start YouTube through DIAL. Both wake the TV, so use them when someone can see the TV. With `--ssh`, the script also checks that music leaves the TV on Plasma and that VacuumTube closes after the launch test.

A person must still test the casts of real apps: AirPlay mirroring from an Apple device, and the YouTube app of a phone.

## Test media

The repository contains no media. Use short samples with the format in the file name, for example `dv-p7-fel-truehd-atmos.mkv`. `tools/make-audio-samples.sh` creates channel checks for each audio format.

## Report a problem

Name the hardware by its type, for example "Intel Alder Lake-N integrated GPU", the version of the distribution and the output of `sudo ./install.sh --check`. Remove addresses and host names from logs before you share them.
