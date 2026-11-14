# Distributions

The project supports the current Fedora release with the KDE Plasma desktop. Other distributions can work, but the project has no package lists and no builds for them. This page lists the rules that keep the scripts portable, and the parts that are special on Fedora.

## Rules for portable scripts

1. The scripts need systemd, Wayland, KDE Plasma 6 and PipeWire. They need no other part of one distribution.
2. Only the shared library in `lib/` runs the package manager. A component lists its packages for each distribution in its own `packages/` folder.
3. The library finds the distribution from `ID` and `ID_LIKE` in `/etc/os-release`.
4. A component that has no package list for the current distribution stops with a clear message. It does not guess package names.
5. The scripts write files only to standard paths: `/etc`, `/usr/local`, `/opt/x86-linux-htpc` and the home folder of the HTPC user.
6. The scripts do not change files that belong to a package. They add drop-in files instead, for example in `/etc/sddm.conf.d/` or `~/.config/pipewire/pipewire.conf.d/`.

## Fedora

### Codecs

Fedora ships FFmpeg and the Intel video driver without the patented codecs. The full versions are in RPM Fusion (a community repository for Fedora).

| Fedora package | RPM Fusion package | Why |
|---|---|---|
| `ffmpeg-free` | `ffmpeg` | HEVC, AC-3, E-AC-3, TrueHD and DTS decode, and the IEC 61937 packer for passthrough |
| `libva-intel-media-driver` | `intel-media-driver` | Hardware decode of HEVC and AVC on Intel |

The install script turns on RPM Fusion and swaps these packages with `dnf swap`.

### Kernel packages

On Fedora, the package `dkms` needs `kernel-devel-matched`. This package can install a newer kernel only in part, without the package `kernel-modules`. The boot loader then starts that kernel, and it has no drivers for the network or the screen. Before you install `dkms` or a `kernel-devel` package, read the package list. If the list has a new `kernel-core`, also install the full `kernel` package of the same version.

The patched kernel of this project is a set of RPM packages with its own release tag. [kernel.md](kernel.md) describes the build.

## Add a distribution

1. Add a package list for the distribution to each component.
2. If `ID_LIKE` does not cover the distribution, add its detection to `lib/`.
3. Add a kernel build for the package format of the distribution.
4. Run `sudo ./install.sh --check` on a machine with the distribution, and fix each failed check.
