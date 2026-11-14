# Audio

The HTPC sends all compressed audio formats unchanged to the AV receiver. This is called passthrough or bitstreaming. The receiver decodes the audio. [hardware.md](hardware.md) lists what the HDMI audio hardware must support.

## Formats

| Format | Transport on HDMI | Notes |
|---|---|---|
| Dolby Digital (AC-3) | IEC 61937 at 48 kHz, 2 channels | |
| Dolby Digital Plus (E-AC-3), also with Atmos | IEC 61937 at 192 kHz, 2 channels | Atmos in E-AC-3 travels inside the stream |
| Dolby TrueHD, also with Atmos | HBR, 8 channels at 192 kHz | |
| DTS | IEC 61937 at 48 kHz, 2 channels | |
| DTS-HD High Resolution | IEC 61937 at 192 kHz, 2 channels | |
| DTS-HD Master Audio, also with DTS:X | HBR, 8 channels at 192 kHz | DTS:X travels inside the stream |
| PCM up to 7.1 | PCM, 8 channels | Decoded audio and audio without a codec |

IEC 61937 is the standard that packs compressed audio into PCM frames. The receiver sees the packed frames and decodes them.

## Audio driver

The HDMI audio driver decides whether passthrough works. Three drivers are possible on Intel:

1. `snd-hda-intel`, the standard driver. It passes all formats, also in the HBR mode.
2. SOF with the IPC4 interface. It passes the stream unchanged on the HDMI devices. The project does not support this way yet.
3. SOF with the IPC3 interface. Its HDMI devices offer only 2 channels at 48 kHz, and its DSP changes the samples. No compressed format reaches the receiver.

The component `audio-driver` selects the driver from `AUDIO_DRIVER` in the configuration file. `hda` selects `snd-hda-intel` on a machine that uses SOF by default. Kernel options select the driver:

| Driver | Kernel option |
|---|---|
| `snd-hda-intel` instead of SOF | `snd_intel_dspcfg.dsp_driver=1` |
| SOF with IPC4 | `snd_sof_pci.ipc_type=1`, and an IPC4 topology that has HDMI devices |

## Path from the player to the receiver

mpv and Kodi open the HDMI device through PipeWire. The configuration of the HDMI device in PipeWire has the codec list `iec958.codecs`. PipeWire passes the IEC 61937 data of these codecs unchanged. If a format is not in the list, the player decodes it to PCM.

The component `audio-passthrough` writes a WirePlumber drop-in for the HTPC user, `~/.config/wireplumber/wireplumber.conf.d/x86-linux-htpc-hdmi.conf`:

| Setting | Value |
|---|---|
| Card profile | The HDMI profile with the most channels: 7.1, then 5.1, then stereo. Decoded audio then reaches the receiver with all its channels. |
| `iec958.codecs` | The formats of `AUDIO_PASSTHROUGH_FORMATS` in `/etc/x86-linux-htpc/htpc.conf`, plus PCM |
| `session.suspend-timeout-seconds` | 0, if `AUDIO_KEEP_HDMI_OPEN=yes`. A receiver needs about a second to lock on to a new stream, and an open device loses no start of a sound. |
| `device.routes.default-sink-volume` | 1.0. The receiver sets the volume, and the default of WirePlumber (40 %) makes decoded sound very quiet. At the first install with this value, the component also sets each HDMI output to the full level. A volume that the user sets later is stored and wins. |

On Fedora, the component also removes the package `pipewire-config-raop`. With it, PipeWire adds each AirPlay speaker of the LAN as a sink. When the TV turns on or off, the receiver sends a hotplug, and the HDMI sink is gone for a moment. PipeWire then moves the stream of the player to an AirPlay speaker, and the stream stays there. The component `remove.sh` does not install the package again.

At the time that PipeWire reads the capabilities of the HDMI port, the receiver must be on. In standby, many receivers pass on the audio list of the TV, often without DTS and with only 2 PCM channels.

mpv sends the formats in its option `audio-spdif` unchanged, for example `audio-spdif=ac3,eac3,dts-hd,truehd`. The value `dts-hd` also covers DTS.

Kodi has its own passthrough configuration in its audio menu.

## Test files

`tools/make-audio-samples.sh` creates channel checks in AC-3, E-AC-3, DTS, TrueHD and PCM 7.1 with FFmpeg. Each file plays a tone on one channel after the other. The receiver shows the format, and each speaker plays in turn. TrueHD uses the same HBR transport as Atmos and DTS-HD Master Audio. FFmpeg cannot write Atmos, DTS-HD or DTS:X, so a test of these labels needs real content.

## Decode fallback

If the hardware cannot send HBR, the player decodes TrueHD and DTS-HD Master Audio to 7.1 PCM. The 7.1 channels reach the receiver without loss, but the Atmos and DTS:X height objects are lost. FFmpeg cannot decode DTS:X objects.
