# Kernel

The stock kernel of a distribution cannot send Dolby Vision or HDR10+ on HDMI. The project builds a kernel with the display patch set of libreelec-x86-DV. All other parts of the system stay the stock packages of the distribution.

## What the patches change

| Area | Files | Change |
|---|---|---|
| Intel display (`i915`, `xe`) | `drivers/gpu/drm/i915/display/` | Dolby Vision signaling, HDR10+ metadata, a check of the HDMI link, deep color for HDR10 |
| HDMI helpers | `drivers/video/hdmi.c`, `include/linux/hdmi.h` | The Dolby Vision vendor InfoFrame (a data packet on HDMI that describes the signal) |
| DRM core | `drivers/gpu/drm/drm_connector.c`, `drm_atomic_uapi.c` and others | The HDR10+ metadata property of the connector |
| EDID modes | `drivers/gpu/drm/drm_edid.c` | Extra 3D modes |

The patches add these properties to the HDMI connector. A player that is the DRM master sets them in its atomic commit (the call that changes the display state).

| Property | Type | Job |
|---|---|---|
| `DVBRIDGE_DV_STANDARD_LAB` | Boolean | Turns on TV-led Dolby Vision for the next frames |
| `HDR10PLUS_METADATA` | Blob | The HDR10+ metadata of the current frame |
| `DVBRIDGE_HDR10_DEEP_COLOR` | Boolean | Asks for 10-bit or deeper output for HDR10 |
| `CB1_LLDV_FORMAT` | Enumeration | The pixel format for low-latency Dolby Vision |

The Intel patch also adds the module parameter `i915.spd_sdi`. It sets the device type that the HTPC reports to the TV. The default value 1 means "digital set-top box". Some TVs select a PC picture mode for the device type "PC".

## Why a full kernel build

DKMS (a system that builds extra kernel modules for each kernel) cannot carry these patches. `drivers/video/hdmi.c` is part of the kernel image, not a module. The patches also change `include/linux/hdmi.h`, and all display drivers use this header. A change to the image and to a shared header needs a new kernel build.

## Patch series

`kernel/patches/series` lists the patches in the order of application. It fits Linux 7.3.

| Line | Meaning |
|---|---|
| `upstream PATH SHA256` | A file of libreelec-x86-DV at the commit in `kernel/upstream.conf`, with its checksum |
| `local FILE` | A patch in `kernel/patches/`. It is an upstream patch with a rebase on Linux 7.3. |

`kernel/apply-series.sh KERNEL_SOURCE_DIR` applies the series to a fresh kernel tree. It downloads the upstream files into `~/.cache/x86-linux-htpc/`, checks each checksum and stops at the first patch that does not apply.

The series contains the shared DRM and HDMI patches and the Intel patches. The project targets Intel GPUs, so the AMD patches of the upstream project are not in the series. The comments at the top of the series list each left-out patch and the reason.

## Build

The build takes the kernel source package of the distribution and adds the patches. So the new kernel has the same configuration and the same patches as the stock kernel of the distribution.

On Fedora, `kernel.spec` applies the file `linux-kernel-test.patch` after the Fedora patches. The file is empty in the Fedora source package. The build puts the series into this file. It also sets `buildid` in `kernel.spec` to the release tag of the project, so the packages install next to the stock kernel. The Fedora patches for Linux 7.3.3 change none of the files that the series changes.

Two scripts build the packages:

| Script | Use |
|---|---|
| `kernel/build-in-container.sh 7.3.3-300.fc45 OUTPUT_DIR` | Builds in a Fedora container with podman. The build machine needs no Fedora of the same release. |
| `kernel/build-fedora.sh 7.3.3-300.fc45 OUTPUT_DIR` | Builds on a Fedora system of the same release. As root, it also installs the build dependencies. |
| `kernel/newest-fedora-kernel.sh 45 OUTPUT_DIR` | Prints the newest kernel build of a Fedora release. It says whether the series fits its Linux version, and whether `OUTPUT_DIR` has the patched packages of this build. |
| `kernel/vbt-hdmi-rate.py VBT [OUTPUT]` | Shows the HDMI rate limit in a VBT of the board firmware. With `OUTPUT`, it writes a copy with the platform maximum. [hardware.md](hardware.md) describes when the copy helps and how the kernel loads it. |

dnf downloads the source package and checks its signature. Inside a container, the crypto API of the host kernel can lack `hmac(sha512)`, which `kernel.spec` needs for the FIPS checksum of the kernel image. Then the build uses `kernel/sha512hmac-fallback.py`, which writes the same checksum. The build leaves out the parts that an HTPC does not use, for example the debug kernel, `perf` and the tools. The build files stay in `~/.cache/x86-linux-htpc/`.

The upstream patch set follows one kernel version. When the distribution moves to a newer kernel, the patches need a rebase. The skill `kernel-patches` describes this work.

## Install and fallback

The component `kernel-dv` does these steps on Fedora. It reads the folder of the built packages from `KERNEL_PACKAGES_DIR` in the configuration file.

1. The first run of `./install.sh kernel-dv` installs the patched kernel next to the stock kernel, with `kernel-devel`. DKMS builds its modules for the new kernel during the install.
2. The run keeps the old default kernel, because `kernel-install` makes each new kernel the default.
3. It sets a one-time boot entry with `grub2-reboot`. The next start uses the patched kernel once.
4. If the patched kernel does not start, the next start uses the default kernel again.
5. After a good start of the patched kernel, the second run of `./install.sh kernel-dv` runs the checks and makes the patched kernel the default.

## Secure Boot

If Secure Boot is on, the firmware starts only signed kernels. A kernel that you build yourself has no signature from the distribution. Sign it with your own key and enroll the key with `mokutil`, or turn off Secure Boot.

## Kernel updates of the distribution

A normal update installs new stock kernels, and Fedora makes each new kernel the default. These kernels have no Dolby Vision patches. The component `kernel-dv` installs the `kernel-install` plugin `/etc/kernel/install.d/96-x86-linux-htpc-default.install`. After the install of a stock kernel, it makes the newest patched kernel the default again. The stock kernel stays in the boot menu as a fallback. A new patched kernel needs a new build of the series. `kernel/newest-fedora-kernel.sh` shows when a build is due. The skill `upgrade-stack` gives the order for the whole stack. dnf can remove an old patched kernel like any other old kernel, and the project does not warn about it yet.

## Out-of-tree modules

Other modules from DKMS, for example a fix for one board, must build for the patched kernel too. If the matching `kernel-devel` package is installed, DKMS builds them when the kernel package installs.
