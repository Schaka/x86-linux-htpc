# Upstream projects

The HTPC uses many external projects. This page lists them and shows how the project gets each one.

| Project | Used for | How the project gets it |
|---|---|---|
| [libreelec-x86-DV](https://github.com/CroqueMr/libreelec-x86-DV) | The kernel display patches, the CB1 engine and the patched Kodi, FFmpeg and libplacebo | The build scripts download a fixed commit. Patches that need a change for the distribution kernel are in this repository. |
| [jellyfin-mpv-shim](https://github.com/jellyfin/jellyfin-mpv-shim) | Cast target and player for Jellyfin | Python package or Flatpak |
| [mpv](https://mpv.io) | Video player | Package of the distribution, and a build with CB1 and the patches in `mpv/patches/` |
| [Kodi](https://kodi.tv) | Optional second player with CB1 | The Kodi version and the patches of libreelec-x86-DV, built by `kodi/build.sh` |
| [LightGBM](https://github.com/lightgbm-org/LightGBM) | The HDR10 AI of CB1 in Kodi | The version of libreelec-x86-DV, built by `kodi/build.sh` |
| [libdisplay-info](https://gitlab.freedesktop.org/emersion/libdisplay-info) | The EDID parser of Kodi | The version of libreelec-x86-DV, built by `kodi/build.sh`, because Fedora 45 has an older version |
| [VacuumTube](https://github.com/shy1132/VacuumTube) | YouTube with the TV interface and HDR | Flatpak from Flathub |
| [KDE Plasma Bigscreen](https://invent.kde.org/plasma/plasma-bigscreen) | TV interface | Package of the distribution |
| [Waydroid](https://waydro.id) | Android container | Package of the distribution |
| [SmartTube](https://github.com/yuliskov/SmartTube) | YouTube app | APK from the GitHub releases |
| [TizenTube Cobalt](https://github.com/reisxd/TizenTubeCobalt) | YouTube app | APK from the GitHub releases |
| [S0undTV](https://github.com/S0und/S0undTV) | Twitch app | APK from the GitHub releases |
| [streamlink-ttvlol](https://github.com/2bc4/streamlink-ttvlol) | The list of public Twitch proxy servers for S0undTV | A link in the configuration file |
| [libcec](https://github.com/Pulse-Eight/libcec) and `v4l-utils` | HDMI-CEC | Packages of the distribution |

## Licenses

The own files of this project are under AGPL-3.0-only. Files that come from another project, or that change another project, keep the license of that project:

| Files | License |
|---|---|
| Patches to the Linux kernel | GPL-2.0-only |
| Patches to mpv, Kodi, FFmpeg or libplacebo | The license of the patched project |
| `.agents/skills/simple-english/` | MIT, see the license file in that folder |
| The CB1 HDR10 model in the Kodi build | GPL-2.0-or-later, by a grant of its author. The build copies its `LICENSE.md` next to the model. |

The parts of libreelec-x86-DV have different licenses for each module. A Kodi build with CB1 is under GPL-3.0-only. The file `docs/cb1/LICENSING.md` of that project lists the terms. The scripts of this project download these parts at build time and do not copy them into the repository.
