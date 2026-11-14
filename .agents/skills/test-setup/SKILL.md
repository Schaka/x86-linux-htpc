---
name: test-setup
description: Use when you test setup.sh and the component install on a fresh Fedora system in a container or a virtual machine. When you change setup.sh, install.sh, lib/ or a package list, also use it.
---

# Test the setup

A fresh system shows the problems that a developer machine hides: a missing package, a wrong order of the components, a question without a default. Test in two steps. A container is fast and tests the questions and the packages. A virtual machine has systemd, a user session and a display manager, and tests the install of the components.

Neither has the HTPC hardware. A check of the HDMI port, the audio passthrough, the DRM card or the TV cannot pass. Write down which checks fail for this reason, and which fail for a real problem.

## Rules

1. Do not use a real HTPC for this test, and do not change the system of the build machine. All changes stay in the container or in the virtual machine.
2. Delete the container and the virtual machine at the end.
3. Write the results into a report for the owner. Copy no logs into the repository.

## 1. Container

1. Start a Fedora container with the repository as a read-only folder:

   ```
   podman run -d --name htpc-setup-test --volume "$PWD:/repo:ro,z" registry.fedoraproject.org/fedora:45 sleep infinity
   podman exec htpc-setup-test useradd -m htpc
   ```

2. Run `./setup.sh --questions` and `./setup.sh --defaults` in it. Each question has a text and a default.
3. Write answers files for three cases into the container: a minimal setup (Jellyfin only), a full setup without Dolby Vision, and a setup with Dolby Vision, Kodi and `SETUP_BUILD=no`.
4. For each case, run `HTPC_CONFIG=/tmp/CASE/htpc.conf /repo/setup.sh --answers FILE --no-install`. Read the components and the written configuration file. Make sure that `config_get` of `lib/htpc.sh` reads each value back unchanged.
5. Run a wrong answer, for example `DISPLAY_MODE=big`, and make sure that the setup stops with a clear message.
6. Install the packages of all components for this Fedora release: for each `components/*/packages/fedora.txt`, run `dnf install --assumeno` with its packages, after `components/codecs/install.sh` turned on RPM Fusion. Each package name must exist.

## 2. Virtual machine

1. Get the Fedora KDE Plasma Desktop or the Fedora Cloud image of the same release. With the cloud image, install the group `kde-desktop-environment` and Plasma Login Manager, and set the graphical target.
2. Start it with QEMU and KVM, 4 GB of memory and 2 CPUs, a virtual GPU (`-device virtio-vga`) and the network. Share the repository with the virtual machine, or copy it in.
3. Make the user `htpc`, and run `sudo ./setup.sh --answers FILE` with a setup without Dolby Vision, Kodi and Waydroid. Waydroid needs the binder module of the host kernel.
4. Read the checks at the end. Restart the virtual machine, and run `sudo ./install.sh --check` again.
5. Make sure that the session of `htpc` starts automatically in Plasma Bigscreen: `loginctl list-sessions` and the process `plasma-bigscreen-wayland`.
6. Run `sudo ./setup.sh --answers FILE` a second time. It changes nothing, and all checks pass that passed before.

## Report

For each step, write down: the command, passed or failed, and for a failure the error and its cause. List the fixes that you made in the repository, each with its reason. Name the checks that need real hardware.
