---
name: kernel-patches
description: Use when you select, rebase, change or build the kernel patch set in kernel/. When the distribution moves to a new kernel version, or when the patched kernel fails to build or to start, also use it.
---

# Kernel patches

[docs/kernel.md](../../../docs/kernel.md) describes why the project needs a patched kernel and which parts the patches change.

## Rebase on a new kernel version

1. Get the kernel source of the distribution for the new version. On Fedora, use the source RPM of the `kernel` package.
2. Apply `kernel/patches/series` in order with `patch -p1 --dry-run` first.
3. For each patch that fails, compare the failing part with the upstream patch in libreelec-x86-DV and with the new kernel code. Fix the patch so that it does the same thing on the new code.
4. Keep the header of each patch: the name of the upstream patch, the upstream commit and the kernel version that the patch fits.
5. Build the kernel. A patch that applies can still break the build.
6. If the upstream project has a newer patch set for the new kernel version, take it instead of your own rebase. Load the skill `sync-upstream`.

## Build

1. Use the build script of the distribution in `kernel/`, for example `kernel/build-fedora.sh`.
2. Give the packages the release tag of the project, so they install next to the stock kernel.
3. Build the matching `kernel-devel` package, so that DKMS modules build for the patched kernel.

## Test

1. Install the packages next to the stock kernel. Keep the stock kernel.
2. Start the patched kernel once with a one-time boot entry. Ask the user before the reboot.
3. Make sure that the HDMI connector has the property `DVBRIDGE_DV_STANDARD_LAB`.
4. Make it the default kernel only after the check passes.

## License

The kernel patches are under GPL-2.0-only. Each patch keeps the copyright and the SPDX lines of the upstream patch.
