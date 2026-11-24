---
name: update-cb1
description: Use when the CB1 engine in libreelec-x86-DV has a new version. Also use it for a change of the CB1 API, of its FFmpeg or libplacebo patches or of the CB1 patches of Kodi. Also use it for a CB1 mpv that does not build or play Dolby Vision after an upstream change.
---

# Update CB1

CB1 comes from the `CB1/` folder of libreelec-x86-DV at `UPSTREAM_COMMIT` in `kernel/upstream.conf`. A new CB1 version is a new upstream commit. That commit can also move the kernel patches. The skill `upgrade-stack` gives the order of all layers. This skill gives the CB1 part in detail.

mpv links the CB1 library `libdvbridge_core.a`, which `cb1/build.sh` builds from the upstream source. The mpv patches contain no CB1 code. They hold only the code between mpv and the CB1 calls, in the order of the Kodi renderer. Most new CB1 versions need no patch change. Two changes need one: a changed call that mpv uses, and a new order or new options of the calls in the Kodi renderer. [docs/cb1-integration.md](../../../docs/cb1-integration.md) describes each CB1 call and its place in mpv.

## Rules

1. If the new commit also changes kernel patches, do the kernel part of `upgrade-stack` first. The kernel gives the connector properties that CB1 needs.
2. Keep the archive of the working CB1 build until the new build passes all tests. The libraries have the absolute run path `/opt/x86-linux-htpc/cb1`, so a renamed folder is not a fallback.
3. Ask the user before each install on the HTPC. A build in a container needs no approval. Load the skill `test-on-hardware` before you touch the machine.
4. Keep the mpv changes as the small patches in `mpv/patches/`, one patch for each topic. Do not copy the mpv source into the repository.

## 1. Find out what changed

Clone libreelec-x86-DV into a temporary folder outside of the repository. Run `git diff OLD NEW` for these paths, with OLD as the current `UPSTREAM_COMMIT`:

| Path | What a change means |
|---|---|
| `config/cb1-versions.json` | The CB1 version. `cb1/build.sh` writes it into `cb1.pc`. |
| `CB1/config/dependencies.json` | New FFmpeg or libplacebo versions. `cb1/build.sh` reads them, so the build follows without a change here. |
| `CB1/patches/ffmpeg/`, `CB1/patches/libplacebo/` | New fields in the side data or in the render parameters. mpv sees them only through CB1, unless a field of `pl_frame` or `mp_image` changes. |
| `CB1/src/*.h`, `CB1/docs/API.md` | The API. Each changed call that mpv uses needs a change in a patch. The table in part 2 names the patch. |
| `packages/multimedia/ffmpeg/package.mk`, the libplacebo `package.mk` | New configure options. Copy them into `cb1/build.sh`, and say why in a comment. |
| `projects/Generic/patches/kodi/kodi-9990-cb1-video.patch` | The Kodi renderer (`DVBridgeGLES.cpp`): the order of the calls, the options before each frame, the HDR10 and FEL paths. mpv follows the same order. |
| `projects/Generic/patches/kodi/kodi-9992-cb1-display.patch` | The connector state (`DVBridgeState.cpp`): the properties and values for each mode |

Read the release notes of the commit. Write down each change that touches a row of the table.

## 2. Change the mpv patches

| Changed part | Patch | Files in mpv |
|---|---|---|
| `dvbridge_render.h`, `dvbridge_placebo.h`, `dvbridge_gl_frame.h`, `dvbridge_dmabuf.h`, the Kodi renderer | `0002-cb1-output.patch` | `video/out/gpu_next/cb1.c`, `video/out/vo_gpu_next.c` |
| `dvbridge_fel.h`, the FEL path of the Kodi renderer | `0003-cb1-fel-decoder.patch` | `video/decode/cb1_fel.c`, `video/decode/vd_lavc.c` |
| The connector state of Kodi, the connector properties of the kernel | `0001-drm-dvbridge-connector-state.patch` | `video/out/drm_atomic.c`, `video/out/drm_common.c` |
| A new field of `pl_frame` or of the FFmpeg side data | `0002` or `0003` | `video/mp_image.c`, `video/mp_image.h` |

`0004-drm-own-vt.patch` does not use CB1. It changes only with mpv.

1. Copy the mpv source from `~/.cache/x86-linux-htpc/mpv/` into a temporary folder. Make it a git repository with one commit.
2. Apply each patch in name order, and make one commit for each patch. Then a change to one patch is a change to one commit.
3. Change the code. Load the skill `write-code`. A call that CB1 renamed or extended changes in the same way as in the Kodi renderer.
4. Write each patch again with `git diff` between its commit and the commit before it. Keep the text header of the patch and its license line. If the patch does something new, change its header.
5. Make sure that all patches apply in name order to a clean copy, with `patch -p1 --dry-run`.

## 3. Build

1. Run `cb1/build-in-container.sh FEDORA_RELEASE OUTPUT_DIR` in the background. It builds FFmpeg, libplacebo, CB1 and mpv, because mpv must link the exact new libraries.
2. `cb1/build.sh` runs `tools/verify.py` of CB1 first. If it fails, the upstream tree is not complete. Do not go on.
3. Make sure that `opt/x86-linux-htpc/cb1/libreelec-x86-DV-commit` in the archive holds the new commit: `tar -xzOf ARCHIVE opt/x86-linux-htpc/cb1/libreelec-x86-DV-commit`.
4. Make sure that the new mpv knows the options of the patches: `mpv --list-options` must show `--cb1`, `--drm-dvbridge`, `--drm-master-wait` and `--vd-lavc-dovi-fel`.

## 4. Install

Ask the user. On the HTPC:

1. Make an archive of the working folder: `tar -czf ARCHIVE_OF_OLD -C / opt/x86-linux-htpc/cb1`.
2. Remove the folder `/opt/x86-linux-htpc/cb1`, so that no old library stays. Extract the new archive with `tar -xzf ARCHIVE -C /`.
3. If a test in part 5 fails, remove the folder again and extract the old archive.

## 5. Tests

First run each test with `--no-audio`, and do not ask the user yet. Read the mpv log, the DRM state (`drm_info`, `/sys/kernel/debug/dri/0/i915_display_info`) and the frame drop counters. When these show a working output, ask the user to look at the TV.

| Sample | Options | The mpv log must show |
|---|---|---|
| Profile 5 and profile 8.1 | `--cb1=dolby-vision --drm-dvbridge=dolby-vision --dither-depth=no` | `CB1 Dolby Vision engine started, output Dolby Vision.` |
| Profile 7 FEL | the same and `--vd-lavc-dovi-fel` | `CB1 enhancement layer: FEL`, and no `Dolby Vision FEL` error |
| Profile 7 MEL without an enhancement layer stream | the same and `--vd-lavc-dovi-fel` | `Dolby Vision MEL: the stream has no enhancement layer pictures`, and no `Dolby Vision FEL` error |
| Profile 5 in the HDR10 mode | `--cb1=hdr10 --drm-dvbridge=hdr10 --drm-format=xrgb2101010` | No CB1 error. `drm_info` shows `HDR_OUTPUT_METADATA` with PQ. |
| HDR10 and SDR | none of the CB1 options | No CB1 line. HDR10 sets `HDR_OUTPUT_METADATA`. |

Run all samples with `--vo=gpu-next --gpu-api=opengl --gpu-context=drm --hwdec=vaapi` on a free VT. The frame drop count (`frame-drop-count` and `vo-drop-frame-count`) must stay at 0 for a sample whose frame rate matches the display mode.

If the HTPC uses `JELLYFIN_MPV=cb1`, also cast one Dolby Vision file and one SDR file from a Jellyfin client. The shim log and the mpv log must show `Output for this video:` with the right CB1 mode, and the screen must return to Plasma after each stop.

Then ask the user whether the TV shows Dolby Vision for the Dolby Vision samples, and whether it shows the right picture after the end. Ask all questions in one question after the run.

## 6. Documents

1. [docs/cb1-integration.md](../../../docs/cb1-integration.md): the call tables, the connector state tables and the table "The places in mpv", for each change.
2. [docs/upstream.md](../../../docs/upstream.md): If the page names the CB1 version, change it.
3. [docs/status.md](../../../docs/status.md): if the change needs a new test on the TV, mark the feature as not tested again.
