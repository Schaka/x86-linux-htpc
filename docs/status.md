# Status

This page lists the features of version 1.0.0-beta6. [hardware.md](hardware.md) lists the hardware that the features work on.

## Features

| Feature | Status | Notes |
|---|---|---|
| Plasma Bigscreen with automatic login, home screen tiles, no screen lock and no suspend | Works | |
| Casts from Jellyfin clients to `jellyfin-mpv-shim`, and its library browser | Works | |
| Dolby Vision profile 5, 8.1 and 7 FEL with the CB1 mpv | Works | 4K up to 60 Hz. Needs the patched kernel. |
| HDR10 and SDR with the CB1 mpv | Works | |
| HDR10+ | Not verified | Needs a TV with HDR10+ |
| Refresh rate for each video | Works | In the CB1 mpv, the mpv of the distribution, Kodi and VacuumTube |
| Audio passthrough of Dolby Digital, Dolby Digital Plus, Dolby TrueHD, Dolby Atmos, DTS and DTS-HD, and PCM 7.1 | Works | With `snd-hda-intel`. SOF with IPC3 passes no compressed audio. |
| YouTube with HDR in VacuumTube | Works | 4K HDR at 60 fps drops frames on an N100 class GPU |
| SmartTube in Waydroid | Works | SDR only |
| S0undTV for Twitch in Waydroid, with a proxy against the stream ads | Works | The previews on the home screen of S0undTV do not use the proxy, and can show ads |
| Kodi with CB1 and the add-on "Jellyfin for Kodi" | Works | Dolby Vision profile 5, 8.1 and 7 FEL, and Dolby Atmos |
| Air mouse: OK as a click or as Enter, Back as Back | Works | |
| Wake-on-LAN of the TV at login and at each video | Works | The TV must support a network wake from standby |
| Power key of the remote turns the TV and the AV receiver on and off | Works | [power-key.md](power-key.md). The HTPC itself stays on. Both routes work: a Home Assistant webhook, and the HTPC alone with Wake-on-LAN and commands. The HTPC alone needs a command for each device ([device-commands.md](device-commands.md)). A remote whose power button works by infrared needs another key. |
| `setup.sh` on a fresh Fedora KDE install | Works | |
| REST API with an OpenAPI document, to start YouTube videos, Jellyfin items, media addresses and apps, and to control the player that plays | Works | [control-api.md](control-api.md). The Android apps start, but report no state. |
| DLNA renderer for Home Assistant and DLNA apps | Works | No volume, no queue. Music from a DLNA app plays without a picture. |
| YouTube casting from the YouTube app of an Android phone | Works | The control API answers DIAL while VacuumTube is closed, starts it and sends the launch on. The phone must link the HTPC once, and then lists it under "Linked devices". [control-api.md](control-api.md) |
| AirPlay receiver with UxPlay, for audio and screen mirroring from iPhones and Macs | Not verified | [airplay.md](airplay.md). Needs an Apple device. The receiver shows up in the network. |
| Home Assistant | Guide | [home-assistant.md](home-assistant.md) |
| HDMI-CEC | Planned | [cec.md](cec.md) has the design |

## Known limits

1. The mpv of the distribution drops about half of the frames at 4K with 50 and 60 Hz on an N100 class GPU. The CB1 mpv does not have this problem, so `JELLYFIN_MPV=cb1` is the better choice on this hardware.
2. Plasma keeps one output mode for all windows. The Android apps of Waydroid get only SDR video.
3. The firmware of some boards limits the HDMI port to 297 MHz, so 4K at 50 and 60 Hz runs only as YCbCr 4:2:0. [hardware.md](hardware.md) shows the fix with a copy of the VBT.
4. The builds of CB1, Kodi and the kernel exist only for Fedora.
