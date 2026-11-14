---
name: upgrade-stack
description: Use when a new kernel is out, either a Fedora kernel build or a Linux version that the patch series must follow. Also use when libreelec-x86-DV publishes a commit, when the version of FFmpeg, libplacebo, CB1 or mpv moves, or when the user asks to upgrade the whole stack.
---

# Upgrade the stack

The Dolby Vision stack has layers, and each layer depends on the layer below it. This skill gives the order of the upgrade. The single steps are in other skills: `kernel-patches` for the patch series, `update-cb1` for CB1 and the mpv patches, `sync-upstream` for the external projects, `test-on-hardware` for the machine and `write-docs` for the documents. [docs/kernel.md](../../../docs/kernel.md), [docs/upstream.md](../../../docs/upstream.md) and [docs/cb1-integration.md](../../../docs/cb1-integration.md) describe the parts.

## The layers

| Layer | Where its version is fixed | Built by |
|---|---|---|
| Kernel | The Fedora kernel build is the argument of the build script. The series fits one Linux version, named in the header of `kernel/patches/series`. | `kernel/build-in-container.sh` |
| libreelec-x86-DV | `UPSTREAM_COMMIT` in `kernel/upstream.conf`. The kernel series and `cb1/build.sh` both read it. | Not built |
| FFmpeg, libplacebo, CB1 | `CB1/config/dependencies.json` and the `CB1/` folder of the upstream tree at `UPSTREAM_COMMIT` | `cb1/build-in-container.sh` |
| mpv | `mpv_commit` in `mpv/build.sh` | `cb1/build-in-container.sh` |

Each layer depends on the ones above it in this list:

1. The series depends on the Linux version and on the upstream commit.
2. FFmpeg, libplacebo and CB1 depend on the upstream commit.
3. The patches in `mpv/patches/` depend on the mpv version, on the CB1 headers, on the versions of FFmpeg and libplacebo, and on the connector properties of the kernel.

## Rules

1. Change one layer, test it, then go to the next layer. If a test fails, the failure then names its layer. The order is: kernel, then FFmpeg, libplacebo and CB1, then mpv, then the playback tests.
2. Ask the user before each install, kernel change, boot loader change and reboot on the HTPC. A build in a container changes nothing on the HTPC and needs no approval. Load the skill `test-on-hardware` before you touch the machine.
3. Keep the working state as a fallback until the new state passes its tests.
4. A change of `UPSTREAM_COMMIT` moves the kernel series and the CB1 stack at the same time. Do both parts.

## 1. Find out what moved

1. For the kernel, run `kernel/newest-fedora-kernel.sh FEDORA_RELEASE PACKAGES_DIR`. `FEDORA_RELEASE` is the Fedora release of the HTPC, for example 45. `PACKAGES_DIR` is the folder with your last build. The script prints the newest kernel build of that release, for example `7.3.3-300.fc45`. On stderr, it says whether the series fits the Linux version of that kernel, and whether `PACKAGES_DIR` has the patched packages of that build.
2. Without the script, run `dnf repoquery --releasever=FEDORA_RELEASE --latest-limit=1 --queryformat '%{version}-%{release}\n' kernel`. Add `--enablerepo=updates-testing` to see a build that is still in testing. The build script downloads the source package with dnf, so it can build only a kernel that a repository offers. If a build exists only in Koji, wait until it reaches a repository.
3. On the HTPC, `uname -r` and `rpm -q kernel-core` list the kernels. A release with `.htpc.` in its name is a patched kernel.
4. For libreelec-x86-DV, ask the owner of the repository which branch to follow. Compare its head with `UPSTREAM_COMMIT`.
5. For the CB1 stack, the file `/opt/x86-linux-htpc/cb1/libreelec-x86-DV-commit` holds the commit of the installed build. Compare it with `UPSTREAM_COMMIT`.
6. For mpv, `mpv/build.sh` builds the mpv version that the Fedora release of the HTPC ships. Its comment names the version. Find the version of the Fedora release with `dnf repoquery --releasever=FEDORA_RELEASE --latest-limit=1 --queryformat '%{version}\n' mpv`.

## 2. A new commit of libreelec-x86-DV

Skip this part if the commit does not move.

1. Clone the upstream project into a temporary folder outside of the repository. Read the release notes. Run `git diff --stat OLD NEW` for the paths that this repository reads: `packages/linux/patches/default/`, `projects/Generic/patches/linux/`, `projects/Generic/patches/ffmpeg/`, `packages/addons/addon-depends/multimedia-tools-depends/libplacebo/patches/`, `packages/multimedia/ffmpeg/package.mk`, `config/` and `CB1/`.
2. Change `UPSTREAM_COMMIT` in `kernel/upstream.conf`.
3. Update `kernel/patches/series`:
   1. For each `upstream` line, write the new SHA256 of the file at the new commit (`git show NEW:PATH | sha256sum`). Remove the line if upstream removed the file.
   2. Add each new upstream patch that changes the display or the HDMI code of Intel GPUs, in the order of the file names. Leave out the groups that the comment at the top of the series names, and update that comment if a left-out patch changes.
   3. For each `local` line, compare the upstream original at the new commit with the base of the local patch. If upstream changed it, rebase the local patch again with the skill `kernel-patches`, and write the new commit into its `Upstream:` header line.
4. Run `kernel/apply-series.sh --list`. It downloads each upstream file and tests each checksum. It needs no kernel source. The command must list all files without an error.
5. Read the diff of the configure options in `packages/multimedia/ffmpeg/package.mk` and in the libplacebo `package.mk` of the upstream project. `cb1/build.sh` copies these options. If they changed, change `cb1/build.sh` in the same way and say why in a comment.
6. Go on with part 3. Then do part 4 and part 5, because the commit also moves the CB1 stack.

## 3. The kernel

1. If the Linux version of the kernel has a different minor version than the series (the `X.Y` in the line `It fits Linux X.Y.`), the patches need a rebase. Also rebase if a Fedora stable update breaks a patch. Load the skill `kernel-patches` and follow "Rebase on a new kernel version". If upstream has a series for the new Linux version, take it instead.
2. `kernel/build-fedora.sh` applies the series to the vanilla source before it starts `rpmbuild`. A patch that does not apply stops the build within a short time and is named in the message. The rebuilt patch goes into `kernel/patches/` as a `local` line of the series, with the header that `kernel-patches` describes. Write the Linux version into the line `It fits Linux X.Y.` of the series, because `kernel/newest-fedora-kernel.sh` reads it.
3. Make an empty folder for this build. The component `kernel-dv` takes the first matching package in its folder, so the folder must hold the packages of one build only.
4. Run the build in the background, because it takes from 30 minutes to some hours: `kernel/build-in-container.sh KERNEL_VERSION_RELEASE OUTPUT_DIR`. For example, use `7.3.3-300.fc45`. On a Fedora system of the same release, `kernel/build-fedora.sh` takes the same arguments. If `rpmbuild` stops in the prepare step at the file `linux-kernel-test.patch`, a Fedora patch changes a file of the series. Hand this over to `kernel-patches`.
5. When the build ends, make sure that `OUTPUT_DIR` has these packages with `.htpc.` in their names: `kernel`, `kernel-core`, `kernel-modules`, `kernel-modules-core`, `kernel-modules-extra` and `kernel-devel`.
6. Make sure that the patches are in the build. In an empty folder, extract the module with `rpm2cpio PACKAGE | cpio -idm '*/i915.ko*'` for each `kernel-modules*` package. Then run `modinfo -F parm` on the file. The output must have the parameter `spd_sdi`.
7. Ask the user. Then follow [docs/kernel.md](../../../docs/kernel.md), section "Install and fallback":
   1. Copy the packages to the folder that `KERNEL_PACKAGES_DIR` names in the configuration file of the HTPC. If the folder is a different one, tell the user to change the key.
   2. Run `./install.sh kernel-dv`. The run installs the packages next to the stock kernel, builds the DKMS modules, keeps the old default kernel and sets a one-time boot entry.
   3. If the Fedora build of the packages is the build of the patched kernel that is already installed, the package names are equal and the run skips them. Do not force it. Tell the owner, who decides between waiting for the next Fedora kernel build and removing the installed patched kernel first.
   4. Run `rpm -q kernel-core`. dnf keeps 3 kernels, so make sure that a stock kernel and the last working patched kernel are still installed.
8. Ask the user before the reboot. The reboot starts the patched kernel once. If it does not start, the next start uses the old default kernel, which stays the default until the checks pass.
9. After the start, run `components/kernel-dv/check.sh`. It makes sure that the kernel is the patched kernel, that the connector has `DVBRIDGE_DV_STANDARD_LAB` and `HDR10PLUS_METADATA`, and that DKMS built all modules. Also run `drm_info` and make sure that the HDMI connector has `DVBRIDGE_HDR10_DEEP_COLOR` and `CB1_LLDV_FORMAT`. Then run `./install.sh --check` for the checks of all other components.
10. If all checks pass, run `./install.sh kernel-dv` again. It makes the patched kernel the default. The stock kernel stays in the boot menu as the fallback.

## 4. FFmpeg, libplacebo and CB1

1. `cb1/build.sh` reads the pinned versions from the upstream tree at `UPSTREAM_COMMIT`. The FFmpeg version, its URL and its checksum, and the libplacebo revision come from `CB1/config/dependencies.json`. The CB1 version comes from `config/cb1-versions.json`. This repository needs no other change for a new version.
2. Load the skill `update-cb1`. It lists the upstream files to compare, the mpv patch for each changed CB1 call, and the CB1 tests. Then continue here.
3. Run the build in the background: `cb1/build-in-container.sh FEDORA_RELEASE OUTPUT_DIR`. It builds FFmpeg, libplacebo, CB1 and mpv, because mpv must link the exact new libraries. The result is `OUTPUT_DIR/x86-linux-htpc-cb1-fcFEDORA_RELEASE.tar.gz`.
4. Make sure that the file `opt/x86-linux-htpc/cb1/libreelec-x86-DV-commit` in the archive holds `UPSTREAM_COMMIT`. Use `tar -xzOf ARCHIVE opt/x86-linux-htpc/cb1/libreelec-x86-DV-commit`.
5. Keep the archive of the working build. The libraries have the absolute run path `/opt/x86-linux-htpc/cb1`, so a renamed old folder is not a fallback. The mpv of the distribution stays the fallback player.
6. Ask the user. Then remove the old folder `/opt/x86-linux-htpc/cb1`, so that no old library stays, and extract the archive with `tar -xzf ARCHIVE -C /`.

## 5. mpv

1. If the mpv of the Fedora release moved, find the commit of its tag with `git ls-remote https://github.com/mpv-player/mpv 'refs/tags/vVERSION*'`. The line with `^{}` holds the commit for an annotated tag. Otherwise use the plain line. Change `mpv_commit` and the version in the comment of `mpv/build.sh`.
2. The build applies each file of `mpv/patches/` and stops at the first patch that does not apply. Copy the mpv source from `~/.cache/x86-linux-htpc/mpv/` into a temporary folder and run `patch -p1 --dry-run` with each patch in name order. Fix each failing hunk against the new mpv code. Fix a call of CB1 that changed in the same way. The table "The places in mpv" in [docs/cb1-integration.md](../../../docs/cb1-integration.md) names the files. Keep the header of each patch, with its license line. Load the skill `write-code`.
3. If the connector properties of the kernel changed, change `0001-drm-dvbridge-connector-state.patch` too. [docs/kernel.md](../../../docs/kernel.md) lists the properties.
4. Build with `cb1/build-in-container.sh`, as in part 4. A new mpv version can need a newer FFmpeg or libplacebo. In this case, `meson setup` names the missing version in its error message.

## 6. Tests

Load the skill `test-on-hardware`, and ask the user before each test that needs the machine or a look at the TV.

1. After the kernel start, make sure that SDR and HDR10 playback with the mpv of the distribution works in Plasma. [docs/testing.md](../../../docs/testing.md) has the table of checks.
2. After the installation of the CB1 archive, play an HDR10 sample with the new mpv, with `gpu-api=opengl` and `gpu-context=drm` on a free VT.
3. Play a Dolby Vision sample of profile 5 and one of profile 8.1 with the options of the patches (`--cb1=dolby-vision` and `--drm-dvbridge=dolby-vision`). Ask the user if the TV shows Dolby Vision, and if it shows SDR again after the end of playback.
4. Play a profile 7 FEL sample with `--vd-lavc-dovi-fel` as well. Make sure that the mpv log shows `CB1 enhancement layer: FEL` and no `Dolby Vision FEL` error.
5. If one test fails, go back to the layer below, and test that layer alone with the old layers above it.

## 7. Update the documents

The documents state the status quo, so change each fact that the upgrade made wrong.

1. `kernel/patches/series`: the line `It fits Linux X.Y.` and the comment about patches that the kernel contains.
2. [docs/kernel.md](../../../docs/kernel.md): the Linux version of the series, the example version of the build commands and the sentence about the Fedora patches.
3. [docs/cb1-integration.md](../../../docs/cb1-integration.md): the versions of FFmpeg and mpv, the table of connector properties, the call table and the places in mpv, if they changed.
4. `mpv/build.sh`: the comment with the mpv version.
5. [docs/hardware.md](../../../docs/hardware.md): change each fact about a hardware type in the table "Known hardware". Write facts, as the skill `write-docs` describes.
6. [docs/status.md](../../../docs/status.md): the status of each feature that the upgrade touched.

A status says what is true for the code in the repository. If the upgrade changed the code of a feature and nobody tested it again, the feature is not tested.
