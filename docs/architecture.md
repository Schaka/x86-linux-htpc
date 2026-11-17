# Architecture

This page describes the parts of the HTPC and how they work together. [status.md](status.md) shows which parts work and on which hardware.

## Layers

| Layer | Part | Job |
|---|---|---|
| Kernel | Linux with the display patch set of [libreelec-x86-DV](upstream.md) | Sends the Dolby Vision and HDR10+ signals on HDMI. The stock kernel cannot do this. |
| Audio | ALSA (the kernel sound layer) and PipeWire (the sound server) | Sends compressed audio unchanged to the AV receiver |
| Desktop | KDE Plasma Bigscreen on KWin (the Plasma compositor) | The TV interface. It starts the players and the apps. |
| Players | `jellyfin-mpv-shim` with mpv, and Kodi with the CB1 engine | Play the video and the audio |
| Apps | Waydroid (Android in a container) | Runs the Android TV apps for YouTube and Twitch |
| Control | A USB-CEC adapter with libcec | Turns the TV and the receiver on and off, and reads the keys of the TV remote |
| Remote control | `htpc-control`, a REST API and DLNA renderer ([control-api.md](control-api.md)) | Lets Home Assistant and phones start media on the HTPC and control it |
| Install | Shell scripts in this repository | Install and check each part |

## Who owns the screen

Only one program at a time can control the display hardware. Linux calls this program the DRM master (DRM is the kernel display interface). In a normal session, KWin is the DRM master. All other programs give their pictures to KWin, and KWin puts them together on the screen.

KWin can send HDR10 and HLG to the TV. A player inside Plasma tells KWin the HDR format of the video, and KWin sends the matching signal. This is enough for SDR, HLG and HDR10.

KWin cannot send Dolby Vision. The Dolby Vision mode of this project is "TV-led": the player puts the Dolby Vision data into the pixels of an 8-bit RGB picture, and the TV decodes it. If any program changes one pixel value, the TV shows a wrong picture. Color management and window mixing in KWin change the pixel values. KWin also has no way to set the special HDMI signal for Dolby Vision. HDR10+ has the same problem, because it needs metadata for each frame that KWin cannot send.

For this reason the Dolby Vision player runs outside of Plasma. It runs on its own virtual terminal (VT, a separate console of the system). When the system switches to that VT, KWin gives up the display, and the player becomes the DRM master. When the player stops, the system switches back to the VT of Plasma, and KWin takes the display again. systemd-logind (the session manager) moves the display access between the two sessions.

## Players

| Player | Dolby Vision and HDR10+ | SDR, HLG and HDR10 |
|---|---|---|
| `jellyfin-mpv-shim` with `JELLYFIN_MPV=cb1` | The CB1 mpv on its own VT | The CB1 mpv on its own VT |
| `jellyfin-mpv-shim` with `JELLYFIN_MPV=system` | As HDR10, without the dynamic metadata | The mpv of the distribution, inside Plasma |
| Kodi with CB1, optional | Kodi on its own VT | Kodi on its own VT |

CB1 is the open-source Dolby Vision engine of libreelec-x86-DV. Its project uses it inside Kodi. This project also ports it to mpv with the patches in `mpv/patches/`. Both mpv and Kodi draw the video with libplacebo (a video rendering library), and CB1 works through libplacebo. [players.md](players.md) has the details.

## Flows

### Play from Jellyfin with the CB1 mpv

1. The user picks a video in a Jellyfin client, for example the phone app, and casts it to the HTPC.
2. `jellyfin-mpv-shim` receives the request and starts the CB1 mpv. The HTPC sends Wake-on-LAN to the TV.
3. A script of mpv picks the output for the file: Dolby Vision with CB1, or the HDR or SDR output of mpv, and the display mode with the frame rate of the video.
4. mpv asks logind to switch to its VT. logind takes the DRM master from KWin, and mpv gets it.
5. mpv sends the compressed audio unchanged to the HDMI audio device through PipeWire.
6. After the video, a helper switches back to the VT of Plasma and shows the Bigscreen home screen.

### Play from Jellyfin with the mpv of the distribution

1. `jellyfin-mpv-shim` plays the video in mpv full screen inside Plasma.
2. A script of mpv switches the display mode of the TV to the frame rate of the video.
3. mpv tells KWin the HDR format of the video. KWin sends HDR10 or SDR to the TV.

### Play in Kodi

1. The user opens the Kodi tile in Plasma Bigscreen.
2. A launcher switches to the VT of Kodi and starts Kodi. Kodi becomes the DRM master and plays the video with CB1.
3. When the user leaves Kodi, the launcher switches back to Plasma.

### Remote key

1. The user presses a key on a USB remote or an air mouse.
2. For an air mouse, a service turns OK and Back into a click, Enter or Back.
3. The program in front receives the key: Plasma Bigscreen, an app, Kodi, or a helper of the CB1 mpv that reads the keys while the VT of mpv is in front.
4. The power key goes to a service that turns the TV and the AV receiver on or off. The HTPC stays on. [power-key.md](power-key.md) has the details.

HDMI-CEC from the TV remote is a design without a component yet. [cec.md](cec.md) has the details.

## Configuration of one machine

The repository contains no personal data and no configuration of one household. Each machine keeps its own configuration in `/etc/x86-linux-htpc/htpc.conf`. The install scripts read this file. Examples of its values are the user of the HTPC session, the address of the media server and the audio formats of the receiver.
