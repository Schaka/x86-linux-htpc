#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Finds out which keys a USB or Bluetooth remote sends, and whether it needs
the air mouse service of the component remote-keys.

  tools/detect-remote.py [--timeout SECONDS]

Run it on the HTPC as the HTPC user, or as root. The user must be in the
group "input". Press the keys in this order. The script takes the next key
press as the next key, so a person at the TV can follow the order from a
list and does not need to see the screen of the script.

  1. With the pointer off: OK, Back, Home, Menu, Up, Down, Left, Right,
     Play/Pause, Volume up, Volume down.
  2. Turn the pointer on, move it for about 2 seconds, then press OK.
  3. Put the remote down and do not touch it for 8 seconds.

At the end, the script prints a summary and a row for the table "Known air
mice" in docs/hardware.md. The first key, OK, shows which receiver belongs
to the remote. After it, the script takes the input devices of that
receiver for itself, so that the other keys do not reach Plasma. A keyboard
of the computer stays free.

  --timeout SECONDS  how long the script waits for each key (default 20).
                     A key that does not come in time counts as missing.
"""
import argparse
import errno
import fcntl
import os
import re
import select
import struct
import sys
import time

EVENT_FORMAT = "llHHi"
EVENT_SIZE = struct.calcsize(EVENT_FORMAT)
EV_KEY, EV_REL = 1, 2
KEY_PRESS = 1
# _IOW('E', 0x90, int) of <linux/input.h>
EVIOCGRAB = 0x40044590
# Bus types of <linux/input.h>
BUS_USB, BUS_BLUETOOTH = 0x03, 0x05
SERVICE_DEVICE_NAME = "x86-linux-htpc air mouse"
SERVICE = "x86-linux-htpc-air-mouse.service"
KEY_HEADER = "/usr/include/linux/input-event-codes.h"

KEYS_WITHOUT_POINTER = ["OK", "Back", "Home", "Menu", "Up", "Down", "Left", "Right",
                        "Play/Pause", "Volume up", "Volume down"]
# The hand needs a moment to put the remote down before the quiet time.
PUT_DOWN_TIME = 3.0
QUIET_TIME = 5.0

# The names for the keys of remotes, for a system without the kernel headers
FALLBACK_NAMES = {
    1: "KEY_ESC", 14: "KEY_BACKSPACE", 28: "KEY_ENTER", 96: "KEY_KPENTER",
    102: "KEY_HOME", 103: "KEY_UP", 105: "KEY_LEFT", 106: "KEY_RIGHT",
    108: "KEY_DOWN", 113: "KEY_MUTE", 114: "KEY_VOLUMEDOWN", 115: "KEY_VOLUMEUP",
    116: "KEY_POWER", 127: "KEY_COMPOSE", 139: "KEY_MENU", 142: "KEY_SLEEP",
    158: "KEY_BACK", 163: "KEY_NEXTSONG", 164: "KEY_PLAYPAUSE",
    165: "KEY_PREVIOUSSONG", 166: "KEY_STOPCD", 172: "KEY_HOMEPAGE",
    174: "KEY_EXIT", 207: "KEY_PLAY", 217: "KEY_SEARCH", 272: "BTN_LEFT",
    273: "BTN_RIGHT", 274: "BTN_MIDDLE", 352: "KEY_OK", 353: "KEY_SELECT",
}
BTN_LEFT, BTN_RIGHT = 272, 273
ENTER_KEYS = {28, 96, 352, 353}


def key_names():
    names = dict(FALLBACK_NAMES)
    try:
        with open(KEY_HEADER) as f:
            for line in f:
                match = re.match(r"#define\s+((?:KEY|BTN)_\w+)\s+(0x[0-9a-fA-F]+|\d+)\b", line)
                if match:
                    # The first name of a code is the main one, for example
                    # BTN_LEFT and not BTN_MOUSE.
                    names.setdefault(int(match.group(2), 0), match.group(1))
    except OSError:
        pass
    return names


NAMES = key_names()


def key_name(code):
    return NAMES.get(code, f"code {code}") if code is not None else "missing"


def read_sysfs(path):
    try:
        with open(path) as f:
            return f.read().strip()
    except OSError:
        return ""


class InputDevice:
    def __init__(self, event_name):
        base = f"/sys/class/input/{event_name}/device"
        self.path = f"/dev/input/{event_name}"
        self.name = read_sysfs(f"{base}/name")
        self.bus = int(read_sysfs(f"{base}/id/bustype") or "0", 16)
        self.usb_id = f"{read_sysfs(f'{base}/id/vendor')}:{read_sysfs(f'{base}/id/product')}"
        self.fd = None

    def is_candidate(self):
        return self.bus in (BUS_USB, BUS_BLUETOOTH) and self.name != SERVICE_DEVICE_NAME

    def open(self):
        self.fd = os.open(self.path, os.O_RDONLY | os.O_NONBLOCK)

    def grab(self):
        fcntl.ioctl(self.fd, EVIOCGRAB, 1)

    def events(self):
        try:
            data = os.read(self.fd, EVENT_SIZE * 64)
        except BlockingIOError:
            return []
        return [struct.unpack_from(EVENT_FORMAT, data, offset)[2:]
                for offset in range(0, len(data) - EVENT_SIZE + 1, EVENT_SIZE)]

    def close(self):
        if self.fd is not None:
            os.close(self.fd)


def open_devices():
    devices = []
    for event_name in sorted(os.listdir("/sys/class/input")):
        if not event_name.startswith("event"):
            continue
        device = InputDevice(event_name)
        if not device.is_candidate():
            continue
        try:
            device.open()
        except PermissionError:
            sys.exit(f"No access to {device.path}. Add the user to the group input, or run as root.")
        except OSError:
            continue
        devices.append(device)
    if not devices:
        sys.exit("No USB or Bluetooth input device. Plug in the receiver of the remote.")
    return devices


def keep_only_receiver(devices, receiver):
    """Closes the devices of other USB IDs and grabs the devices of the
    receiver. A device that another program grabbed sends no events, so the
    result would be wrong."""
    kept = []
    for device in devices:
        if device.usb_id != receiver.usb_id or device.bus != receiver.bus:
            device.close()
            continue
        try:
            device.grab()
        except OSError as error:
            if error.errno == errno.EBUSY:
                sys.exit(f"Another program holds {device.name}. If it is {SERVICE}, stop it with "
                         f"'sudo systemctl stop {SERVICE}', run this script, and start it again.")
            raise
        kept.append(device)
    return kept


class Recorder:
    def __init__(self, devices):
        self.devices = devices
        self.movements = 0

    def wait(self, timeout, want_key):
        """Reads events until a key press comes, or until the time is over.
        Returns the code and the device of the key, or (None, None)."""
        deadline = time.monotonic() + timeout
        while (left := deadline - time.monotonic()) > 0:
            ready, _, _ = select.select([d.fd for d in self.devices], [], [], left)
            for device in self.devices:
                if device.fd not in ready:
                    continue
                for kind, code, value in device.events():
                    if kind == EV_REL:
                        self.movements += 1
                    elif kind == EV_KEY and value == KEY_PRESS and want_key:
                        return code, device
        return None, None

    def wait_for_key(self, timeout):
        return self.wait(timeout, want_key=True)

    def count_movements(self, seconds):
        self.wait(seconds, want_key=False)
        count, self.movements = self.movements, 0
        return count


def record(recorder, timeout):
    keys = {}
    receiver = None
    print("Turn the pointer of the remote off, if it has a key for that.")
    for label in KEYS_WITHOUT_POINTER:
        print(f"Press {label} ... ", end="", flush=True)
        code, device = recorder.wait_for_key(timeout)
        keys[label] = code
        if device and receiver is None:
            receiver = device
            recorder.devices = keep_only_receiver(recorder.devices, receiver)
        print(f"{key_name(code)} ({device.name})" if device else "missing")

    print("Turn the pointer on, move it for about 2 seconds, then press OK ... ", end="", flush=True)
    recorder.movements = 0
    code, device = recorder.wait_for_key(timeout)
    keys["OK with pointer"] = code
    pointer_moves = recorder.movements
    print(f"{key_name(code)}, {pointer_moves} movement events")

    print(f"Put the remote down and do not touch it for {PUT_DOWN_TIME + QUIET_TIME:.0f} seconds ... ",
          end="", flush=True)
    recorder.count_movements(PUT_DOWN_TIME)
    still_moves = recorder.count_movements(QUIET_TIME)
    print(f"{still_moves} movement events")
    return keys, receiver, pointer_moves, still_moves


def needs_service(keys):
    """The service turns a click into Enter and a right click into Back. A
    remote that sends Enter and Back itself does not need it."""
    return keys["OK"] == BTN_LEFT or keys["Back"] == BTN_RIGHT


def ok_description(keys):
    plain, pointer = keys["OK"], keys["OK with pointer"]
    if plain in ENTER_KEYS and pointer == BTN_LEFT:
        return f"`BTN_LEFT` with the sensor on, `{key_name(plain)}` with the sensor off"
    return f"`{key_name(plain)}`"


def print_summary(keys, receiver, pointer_moves, still_moves):
    usb_id = receiver.usb_id if receiver else "unknown"
    service = needs_service(keys)
    print()
    print(f"Receiver: {receiver.name if receiver else 'unknown'}, USB ID {usb_id}")
    if pointer_moves == 0:
        print("The remote sent no pointer movement. It has no pointer, or the pointer was off.")
    if service:
        print(f"The remote needs the air mouse service: add {usb_id} to AIR_MOUSE_USB_IDS.")
    else:
        print("The remote needs no entry in AIR_MOUSE_USB_IDS.")
    if still_moves:
        print(f"While it lay still, it sent {still_moves} movement events in {QUIET_TIME:.0f} seconds.")

    notes = "Yes." if service else "No."
    if keys["OK"] in ENTER_KEYS and keys["OK with pointer"] == BTN_LEFT:
        notes += " The remote changes OK itself."
    if still_moves:
        notes += " While it lies still, it sends a small movement."
    print()
    print("Row for the table \"Known air mice\" in docs/hardware.md. Put the sales name of the remote")
    print("into the first column:")
    print(f"| NAME | `{usb_id}` | {ok_description(keys)} | `{key_name(keys['Back'])}` | "
          f"`{key_name(keys['Home'])}` | `{key_name(keys['Menu'])}` | {notes} |")


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--timeout", type=float, default=20.0)
    args = parser.parse_args()
    recorder = Recorder(open_devices())
    print("Listening to: " + ", ".join(sorted({f"{d.name} ({d.usb_id})" for d in recorder.devices})))
    try:
        print_summary(*record(recorder, args.timeout))
    finally:
        for device in recorder.devices:
            device.close()


if __name__ == "__main__":
    main()
