---
name: test-on-hardware
description: Use when you run a check or a playback test on a real HTPC machine. When a test result changes docs/status.md or docs/hardware.md, also use it.
---

# Test on hardware

## Before the test

1. Ask the user for the access to the test machine. Do not write the address, the user name or the password into any file of the repository.
2. Ask the user before you reboot the machine, change the kernel, change the boot loader or install packages.
3. Before you install a new kernel, keep a kernel that works, and use a one-time boot entry for the first start. On Fedora, use `grub2-reboot`.
4. You cannot see the TV or hear the receiver. Ask the user before each test that plays a picture or a sound, and ask what the TV and the receiver show.

## During the test

1. Run the `check.sh` of each component that the test covers.
2. For playback tests, follow the table in [docs/testing.md](../../../docs/testing.md). Read the logs and the connector state first. Ask the user only for what a log cannot show.
3. Keep logs and command output in a temporary folder outside of the repository.
4. Do not unload or unbind a kernel driver while other drivers use it. Reboot instead.

## Put the result into the documents

1. If the test changed the status of a feature, change it in `docs/status.md`.
2. Add a new fact about a hardware type to the table "Known hardware" in `docs/hardware.md`. Name the hardware by its type and its chip family, for example "Intel Alder Lake-N with SOF audio". Do not name a product, a household or a serial number.
3. Write facts, not a test report: what works, what does not work, and the limit. Leave out dates, the steps of the test and who saw the result.
4. Copy no log into the repository. Logs contain addresses and device names.
