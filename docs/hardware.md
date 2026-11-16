# Hardware

This page lists what the hardware must do for each goal of the project. The weakest part of the chain decides the result. For example, a Dolby Vision TV shows no Dolby Vision from a PC whose HDMI port cannot send the signal.

## The chain

Connect the parts in this order: the HTPC, then the AV receiver, then the TV. The receiver gets the audio directly from the HTPC and passes the video to the TV.

Another way is to connect the HTPC to the TV. The TV then sends the audio to the receiver over eARC (the audio return channel of HDMI 2.1). For this way, the TV must pass all audio formats unchanged. Many TVs do not pass DTS-HD or DTS:X.

## GPU and HDMI port

| Need | Why |
|---|---|
| A GPU that the patch set of libreelec-x86-DV supports | Only these drivers can send the Dolby Vision and HDR10+ signals. [kernel.md](kernel.md) lists the drivers. |
| An HDMI port that the GPU drives directly | The patch set gives no Dolby Vision through an LSPCON (a chip on the board that converts DisplayPort to HDMI). It also gives none through a DisplayPort-to-HDMI adapter. |
| HDMI 2.0 or newer | 4K at 60 Hz with HDR needs the bandwidth of HDMI 2.0 |
| Hardware video decode for HEVC 10-bit and AV1 through VA-API (the Linux video decode interface) | The CPU of a small HTPC cannot decode 4K HEVC or AV1 in software |

This project supports Intel integrated GPUs from Kaby Lake (7th Core generation) and the Intel N-series. The upstream patch set also supports AMD and NVIDIA GPUs, but this project does not use those patches.

The Dolby Vision engine CB1 needs GPU power. On an Intel N100 class GPU, the "Reference" mode plays 4K at 24 frames per second. Heavier modes, 60 frames per second and open menus can be too much for this class.

## HDMI audio

TrueHD, Dolby Atmos in TrueHD, DTS-HD Master Audio and DTS:X need the HBR mode (High Bit Rate audio). In this mode the HDMI port sends 8 channels at 192 kHz, and the receiver gets the compressed stream unchanged. The HDMI audio hardware and its driver must support this mode.

| Audio driver | HBR passthrough |
|---|---|
| `snd-hda-intel`, the standard HD Audio driver | Supported. On Alder Lake-N it passes all formats, also TrueHD in the HBR mode. |
| SOF (Sound Open Firmware, a firmware for the audio DSP of many laptops and Chromebooks) with the IPC4 interface | SOF passes the stream unchanged on the HDMI devices through ChainDMA. Tiger Lake and newer support IPC4. The project does not support this way yet. |
| SOF with the older IPC3 interface | Not possible. The HDMI devices offer only 2 channels at 48 kHz, and the DSP changes the samples, so no compressed format reaches the receiver. |

On a machine with SOF, the component `audio-driver` makes the kernel use `snd-hda-intel` instead (`AUDIO_DRIVER=hda`). Then the internal speakers and microphones stop working on many laptops and Chromebooks. An HTPC does not need them. [audio.md](audio.md) describes the choices.

If the hardware cannot send HBR, the player can decode TrueHD and DTS-HD to multichannel PCM (uncompressed audio). The receiver then plays the 7.1 channels, but the Atmos and DTS:X height objects are lost.

## HDMI-CEC

The project has no CEC component yet. Network wake turns the TV on, and a USB remote or an air mouse sends the keys. [cec.md](cec.md) describes the design for a USB-CEC adapter and where to put it.

## TV

| Need | Why |
|---|---|
| Dolby Vision with the standard "TV-led" mode | CB1 sends TV-led Dolby Vision. A TV that only supports "low latency" Dolby Vision (LLDV) gets no Dolby Vision. |
| HDR10+ support | Only for HDR10+ output |
| Wake-on-LAN in standby | The HTPC turns the TV on over the network. The option has names such as "Turn on via Wi-Fi" or "network standby". |

## AV receiver

| Need | Why |
|---|---|
| Video passthrough with Dolby Vision and HDR10+ | The receiver must pass these signals to the TV unchanged |
| Decoders for Dolby TrueHD, Dolby Atmos, DTS-HD Master Audio and DTS:X | The HTPC sends these formats unchanged |
| On while the HTPC starts its session | In standby, many receivers pass on the audio list of the TV, often without DTS and with only 2 PCM channels |

## Other parts

| Part | Need |
|---|---|
| Memory | 8 GB. Plasma, the player and Waydroid run at the same time. |
| Storage | 64 GB for the system, the Android image and the apps |
| Network | Wired Ethernet. A 4K remux can need more than 100 Mbit/s. |
| Remote control | A 2.4 GHz air mouse, a USB or Bluetooth remote, or a keyboard. [tv-control.md](tv-control.md) lists the types. |

## Known hardware

| Hardware | Facts |
|---|---|
| Intel Alder Lake-N (N100 class) GPU | `i915` drives the HDMI port directly, without an LSPCON. Dolby Vision, HDR10 and SDR work with the CB1 mpv and with Kodi. VA-API decodes H.264, HEVC Main 10, VP9 and AV1 with `intel-media-driver` from RPM Fusion. |
| Intel Alder Lake-N, `snd-hda-intel` | All formats pass unchanged, also TrueHD with 8 channels at 192 kHz (HBR). PCM 7.1 reaches all channels. |
| Intel Alder Lake-N with SOF audio | SOF runs with IPC3, so no compressed format reaches the receiver. Use `AUDIO_DRIVER=hda`. |
| Intel Alder Lake-N, 4K performance | The CB1 mpv and Kodi play 4K up to 60 Hz. The mpv of the distribution with `gpu-next` drops about half of the frames at 4K 50 and 60 Hz. VacuumTube drops about 30 % of the frames at 4K HDR 60 fps. |
| Boards with a VBT limit of 297 MHz | 4K at 50 and 60 Hz runs only as YCbCr 4:2:0. Dolby Vision works only up to 4K 30 Hz, and HDR10 at 4K only with 8 bits. The next section shows the fix. |
| HDMI 2.1 (FRL) | The patch set and the tested GPUs support HDMI 2.0 rates only |

## Known air mice

The table lists the keys that each remote sends to Linux. "Service" says whether the remote needs its USB ID in `AIR_MOUSE_USB_IDS`. [tv-control.md](tv-control.md) explains the service. For another remote, `tools/detect-remote.py` records its keys and prints a row for this table.

| Remote | USB ID of the receiver | OK | Back | Home | Menu | Service |
|---|---|---|---|---|---|---|
| Rii i25 mini | `1997:2433` | `BTN_LEFT` | `BTN_RIGHT` | `KEY_HOMEPAGE` | `KEY_COMPOSE` | Yes. Without the service, OK is always a click and Back is a right click. |
| Orbsmart AM-1 Pro | `1915:1025` | `BTN_LEFT` with the sensor on, `KEY_ENTER` with the sensor off | `KEY_BACK` | `KEY_HOMEPAGE` | `KEY_COMPOSE` | No. The remote changes OK itself. While it lies still, it sends a small movement about once a second. |

Both remotes send the arrows, Play/Pause and the volume keys as standard keys, for example `KEY_UP` and `KEY_PLAYPAUSE`. The USB ID belongs to the radio chip of the receiver, so other remotes can have the same ID and other keys.

## A firmware limit of the HDMI port

The VBT (Video BIOS Table) of the firmware describes the display ports of the board. Its field "HDMI max data rate" can limit an HDMI port below the rate of the GPU. The `i915` driver obeys this field. If a mode needs more, the driver chooses YCbCr 4:2:0, fewer bits, or no mode at all.

A low value can come from the board wiring, or the maker can set it to be safe. The board itself does not show which one applies, so try the copy as the steps describe. The kernel can load a corrected copy of the VBT from a file:

1. Copy the VBT of the running kernel: `sudo cat /sys/kernel/debug/dri/0/i915_vbt > vbt.bin`.
2. Show the limit with `kernel/vbt-hdmi-rate.py vbt.bin`. If it shows "platform maximum", the firmware sets no limit, and these steps do not apply.
3. Write a copy with the platform maximum: `kernel/vbt-hdmi-rate.py vbt.bin vbt-hdmi.bin`.
4. Install the copy into `/usr/lib/firmware/`. `i915` starts from the initramfs, so add the copy to it with `install_items` in a file in `/etc/dracut.conf.d/`. Run `dracut -f`.
5. Add the boot option `i915.vbt_firmware=` with the path of the copy relative to `/usr/lib/firmware`, for example with `grubby --update-kernel=ALL --args=`.
6. After the reboot, make sure that `/sys/kernel/debug/dri/0/i915_vbt` is the same as the copy.
7. Run a 4K 60 Hz RGB mode. Read the SCDC status of the sink with `i2cget` on the DDC bus of the connector, address 0x54. With the clock and the three data channels locked, register 0x40 is 0x0f. The error counters in registers 0x50 to 0x55 stay at 0 on a clean link.

If the picture fails, remove the boot option with `grubby --remove-args=` and reboot. The kernel marks itself as tainted while the option is set.
