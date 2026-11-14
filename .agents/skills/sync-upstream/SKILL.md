---
name: sync-upstream
description: Use when you update the fixed version of an external project that this repository builds or downloads. Examples are libreelec-x86-DV, CB1, Kodi, FFmpeg, libplacebo and the Android apps. When an upstream release fixes a problem of the HTPC, also use it.
---

# Sync upstream projects

[docs/upstream.md](../../../docs/upstream.md) lists the external projects and how this repository gets each one.

## Steps

1. Read the release notes of the new upstream version. Note the changes that touch the parts this project uses.
2. Change the fixed commit or version in the build script of the part.
3. For libreelec-x86-DV, compare the kernel patches, the CB1 patches for FFmpeg and libplacebo, and the Kodi patches with the old commit. Load the skill `kernel-patches` for the kernel part.
4. Build the changed parts.
5. Run the checks of the affected components. If you have a test machine, load the skill `test-on-hardware`.
6. If a version of a component changed in a way that the documents describe, update the page in `docs/`.
7. If the license of an upstream part changed, update `docs/upstream.md`.

## Rules

1. Download external code at build time. Do not copy external source trees into this repository.
2. Fix each external part to one commit, one tag or one checksum. Do not build from a moving branch.
3. Make sure that each download has a checksum or comes from a signed tag.
