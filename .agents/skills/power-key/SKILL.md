---
name: power-key
description: Use when a user wants the power key of a remote to turn the TV and the AV receiver on and off. Also use it when that key does nothing. You pick a route, find the commands of the devices, and make sure that the key works at the HTPC.
---

# Power key

[docs/power-key.md](../../../docs/power-key.md) describes the two routes. [docs/device-commands.md](../../../docs/device-commands.md) shows how to find the commands of a TV or a receiver.

## Rules

1. The HTPC must stay on. Never press, send or simulate the keys `KEY_POWER`, `KEY_SLEEP`, `KEY_SUSPEND` or `KEY_HIBERNATE` on any machine. Never create an input device for a test. The user presses the key at the remote.
2. Ask the people in the room before you send an off command to a TV or a receiver.
3. Never print a webhook address, a token or a pairing key. Do not write them into this repository. Give such a value to the HTPC through a file with the mode `600`, and delete the file at once.
4. Do not change the automations of the user. Add one automation, so that the user can revert by deleting it.
5. Copy `/etc/x86-linux-htpc/htpc.conf` before you change the route.

## Steps

1. Ask the user which route to use. With Home Assistant, recommend route 1. Without it, use route 2.
2. Find the key that the remote sends. Read the input devices with `python-evdev` and without a grab, and ask the user to press the button. If no key arrives, the button works by infrared. Choose another button, for example Search.
3. For route 1, build the automation as the skill `home-assistant` describes. Put the webhook address into `POWER_WEBHOOK_URL`.
4. For route 2, find the commands of each device with the steps of `docs/device-commands.md`. Test the state command first. It only reads.
5. Write the keys `POWER_KEYS`, `POWER_WEBHOOK_URL`, `POWER_ON_COMMAND`, `POWER_OFF_COMMAND` and `POWER_STATE_COMMAND` with `lib/set-config.py`. Then run `sudo ./install.sh power-key` and make sure that all checks pass.
6. Ask the user to press the key once. Read the result in Home Assistant, in the state command and in `journalctl -u x86-linux-htpc-power-key`. Wait a few seconds between presses.
7. Tell the user how to revert. Delete the automation, restore the copy of the configuration file and run `sudo ./install.sh --remove power-key`.
