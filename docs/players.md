# Players

The HTPC has two players. `jellyfin-mpv-shim` with mpv is the main player. Kodi with the CB1 engine is an optional second player.

## jellyfin-mpv-shim

`jellyfin-mpv-shim` is a cast target for Jellyfin. It appears as a device in all Jellyfin clients. When a client casts a video to it, it plays the video in libmpv (mpv as a library). It reports the playback state back to the server.

| Topic | Design |
|---|---|
| Install | From the Python package index with `pipx`, at a fixed version. Fedora has no package. The Flatpak brings its own mpv, but the shim must start the CB1 mpv. So the project uses `pipx` and the libmpv of the system. |
| Start | A systemd user service in the Plasma session. It starts after the login. |
| mpv | `JELLYFIN_MPV` in the configuration file. `system` uses the libmpv of the distribution inside Plasma. `cb1` uses the CB1 mpv on its own VT for all videos. The next section describes `cb1`. |
| Video output with `system` | `vo=gpu-next`, `gpu-api=vulkan`, `target-colorspace-hint=yes`, hardware decode with VA-API |
| Refresh rate with `system` | The script `htpc-refresh.lua` gives the display the refresh rate of each video, and sets the old mode back after playback. [video.md](video.md) explains it. |
| Hardware decode | The shim sets `hwdec` for each file from its own setting `hwdec`, so the install script sets it to `auto` |
| Audio output | PipeWire, with `audio-spdif` from the audio formats in `/etc/x86-linux-htpc/htpc.conf` |
| Library browser | The shim has a library browser in its mpv window. Its scale (`ui_scale`) is the width of `DISPLAY_MODE` divided by 1920, so the browser has the same size on a 4K TV as on a 1080p TV. The launcher entry "Jellyfin" opens it, and the Home key of the remote hides it. Between uses, the shim waits in the background without a window (`start_minimized`, `allow_background`). |
| Configuration | The install script writes the mpv options of the project to `x86-linux-htpc.conf` in `~/.config/jellyfin-mpv-shim/`. The first line of `mpv.conf` includes this file, so the own lines of the user in `mpv.conf` come later and win. In `conf.json`, the script sets only its own keys and keeps all other values. |
| Login | Jellyfin Quick Connect. `sudo components/jellyfin-mpv-shim/login.sh` shows a code, and the user approves it in another Jellyfin client within 5 minutes. No password goes into a file of the project. |

## jellyfin-mpv-shim with the CB1 mpv

With `JELLYFIN_MPV=cb1`, the shim starts the CB1 mpv of `/opt/x86-linux-htpc/cb1` as a separate process (`mpv_ext`) and controls it over IPC. This mpv plays all videos on its own VT, also SDR and HDR10. The shim creates one mpv and keeps it for each file of a session. It picks the mpv program at its own start. A choice of the mpv for each file needs a large change to the shim. So the project plays all videos with the one mpv that can play all formats.

| Topic | Design |
|---|---|
| VT | mpv uses VT 8, above the VTs 1 to 6 that logind fills with a login prompt. When the DRM output starts, it asks logind to switch to this VT (`--drm-vt`). logind drops the DRM master of KWin at the switch, and mpv waits for the master (`--drm-master-wait`). |
| Back to Plasma | The script `htpc-screen.run` watches mpv. One second after mpv no longer needs the screen, it switches back to the VT that was in front before, or else to the VT of the graphical session. After the end or a crash of mpv, it does the same. This also works when mpv runs as its own systemd service, which ends the script together with mpv. Then it calls the KWin script of the component `bigscreen`. Bigscreen keeps its launch screen of the tile until a Plasma window of the app is active, and the script closes it. [desktop.md](desktop.md) describes the script. |
| Rights | The HTPC user is in the groups `video`, `input` and `audio`. logind gives the DRM card, the input devices and the sound card only to the session in front, and the VT of mpv has no session. Without the group `audio`, PipeWire removes the HDMI sink while the VT of mpv is in front. A new group applies at the next login. |
| Keys | On its own VT, mpv has no window system. `htpc-screen.run` reads the keys of all keyboards and remotes while the VT of mpv is in front, and gives them to mpv. The Back key of a remote stops a video that plays (`q`), and goes back in the browser otherwise (ESC), because the shim uses ESC only in its menus and its browser. |
| Output for each video | The script `htpc-output.lua` runs before mpv opens the decoder and the output. A Dolby Vision video of profile 5, 7 or 8 plays with CB1 in the Dolby Vision mode, on an XRGB8888 plane without dithering. All other videos play with the HDR output of mpv, on a 10-bit plane. The display mode gets the refresh rate nearest to the frame rate of the video. If the display has no such mode, mpv uses the preferred mode. |
| Profile 7 FEL | `vd-lavc-dovi-fel=yes` starts the second decoder for a stream with an enhancement layer |
| On-screen display | During Dolby Vision, CB1 draws no OSD over the picture. The keys still work. |

## YouTube with VacuumTube

The component `youtube-tv` installs VacuumTube from Flathub. VacuumTube shows the TV interface of YouTube in Chromium, as a normal window in Plasma. Its tile is "YouTube" (`htpc-youtube-tv.desktop`).

| Topic | Design |
|---|---|
| Video | Chromium decodes VP9 and AV1 with VA-API, also 10-bit HDR at 4K and 60 fps. |
| HDR | The option `wayland_hdr` of VacuumTube lets Chromium describe HDR video to KWin through the Wayland color management protocol. YouTube offers the HDR version of a video only to a screen in HDR mode, and only to a signed-in account. Without a login, YouTube sent HDR video only encrypted, and VacuumTube has no Widevine. |
| Refresh rate and HDR mode | The launcher `youtube-tv` starts VacuumTube and turns HDR on. Once a second, it reads the frame rate and the color format of the playing video from the player, through the debugging port of Chromium on localhost (`flags.txt` of VacuumTube). It sets the mode with the matching rate, and turns HDR off for an SDR video. After five seconds in the menus, HDR comes back on for the choice of the next video. When VacuumTube ends, `htpc-display.service` sets the mode and HDR from the configuration file. |
| Frame rates | YouTube reports 23.976, 29.97 and 59.94 fps as 24, 30 and 60. So the launcher measures the frame rate in the player, from the media time of 120 shown frames, and picks the nearest TV rate. The measurement takes about 5 seconds of playback. |
| Mode change | During a mode change, the TV shows no picture and the HDMI audio device goes away for a moment. YouTube would pause, and its automatic quality would step down. So the launcher pauses the video, changes the mode, waits 3 seconds, and starts the video again at its highest quality. |
| Limit | In HDR, Chromium draws each frame as a 16-bit float image, and KWin draws the whole screen again. On an Intel N100 class GPU, 4K HDR at 24 and 30 fps plays without dropped frames, and 4K HDR at 60 fps drops about 30 % of the frames. SDR at 60 fps plays without dropped frames. |

## Kodi with CB1

libreelec-x86-DV contains patched versions of Kodi, FFmpeg and libplacebo, and the CB1 engine. Kodi runs with its GBM back end (GBM draws directly on the display, without a compositor). FFmpeg, libplacebo, CB1 and the CB1 mpv go into `/opt/x86-linux-htpc/cb1`, and Kodi goes into `/opt/x86-linux-htpc/kodi`. The patched FFmpeg and libplacebo stay in their folder, so they do not replace the libraries of the distribution.

| Topic | Design |
|---|---|
| Build | `kodi/build.sh` builds the Kodi version of libreelec-x86-DV with its patches for the device Generic: GBM, OpenGL ES, CB1 and the HDR10 AI of CB1. The LibreELEC patches for its own system stay out. The build adds libdisplay-info, LightGBM and Python in the Kodi folder. Fedora has an older libdisplay-info and no LightGBM. Kodi runs its add-ons in the Python that it links, and the add-ons follow the Python version of libreelec-x86-DV, not the newer Python of Fedora. `cb1/build-in-container.sh` runs the build in a Fedora container and packs the archive. |
| Install | The component `kodi-dv` unpacks the archive from `CB1_BUILD_DIR`, and installs the launcher and the tile "Kodi". |
| Start | The launcher asks logind to switch to `KODI_VT` (default 9). logind takes the DRM master from KWin. Kodi then opens the card and gets the DRM master. Kodi refuses GBM while `WAYLAND_DISPLAY` is set, so the launcher starts Kodi without it. While the CB1 mpv has its VT in front, the launcher does not start Kodi. |
| End | When Kodi exits, the launcher switches back to the VT of Plasma and calls the KWin script of the component `bigscreen`. The script closes the launch screen of the tile and shows the home screen. The Home key of a remote ends Kodi (key map `~/.kodi/userdata/keymaps/htpc-remote.xml`). |
| Rights | The VT of Kodi has no login session. Kodi reads the DRM card, the input devices and the sound card through the groups `video`, `input` and `audio` of the HTPC user. |
| Audio | Kodi plays through PipeWire, as the other players do. The component writes the defaults of Kodi in `share/kodi/system/settings/appliance.xml`: passthrough for the formats of `AUDIO_PASSTHROUGH_FORMATS`, 7.1 for decoded sound with a receiver, and no menu sounds. A setting that the user changes in Kodi keeps the value of the user. |
| Media | The add-on "Jellyfin for Kodi" from the repository of Jellyfin. It copies the libraries of the server into the library of Kodi. The views, the search and the resume points of Kodi then work as with local files. It streams the files from the server (playback mode "Add-on"). The login uses Quick Connect. The next section has the steps. |
| HDR10 AI | A small LightGBM model in CB1 estimates the Dolby Vision brightness metadata for each scene of an HDR10 video. CB1 then plays the video in the Dolby Vision mode of the TV. The setting is off by default in Kodi. |

## Jellyfin in Kodi

1. Download `https://repo.jellyfin.org/files/client/kodi/repository.jellyfin.kodi.zip`. In Kodi, open Add-ons, then "Install from zip file", and pick the file. Kodi asks once to allow unknown sources.
2. Open Add-ons, then "Install from repository", then "Jellyfin Kodi Add-ons", then "Video add-ons", then "Jellyfin", and install it.
3. The add-on starts its wizard. Enter the server address, for example `http://192.0.2.10:8096`, and pick Quick Connect. Approve the code in another Jellyfin client.
4. Pick the playback mode "Add-on" and the libraries. The first sync of a large server takes several minutes.

## CB1 in mpv

The CB1 mpv plays Dolby Vision itself, so `jellyfin-mpv-shim` covers all formats. The port has these parts:

1. mpv links the same patched FFmpeg and libplacebo as Kodi with CB1.
2. A hook in the `gpu-next` output gives each frame and its Dolby Vision metadata to CB1. CB1 then draws the TV-led picture. CB1 needs OpenGL ES, so this mpv uses `gpu-api=opengl`.
3. mpv runs with its DRM output (`gpu-context=drm`) on its own VT. It sets the connector property `DVBRIDGE_DV_STANDARD_LAB` while Dolby Vision plays. [kernel.md](kernel.md) lists the properties.
4. A second decoder in mpv gives the second layer of profile 7 FEL files to CB1.
5. `jellyfin-mpv-shim` starts this mpv and plays all videos with it. The section above describes it.

[cb1-integration.md](cb1-integration.md) describes each call and its place in mpv.

