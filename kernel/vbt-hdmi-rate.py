#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
#
# Shows the HDMI rate limit and the HDMI level shift in an Intel VBT (Video
# BIOS Table), and writes a copy with the platform maximum.
#
#   kernel/vbt-hdmi-rate.py VBT
#   kernel/vbt-hdmi-rate.py VBT OUTPUT [--level-shift ENTRY]
#
# The firmware of a board can limit an HDMI port below the rate of the GPU.
# i915 obeys that value. The level shift selects the entry of the voltage
# swing table of the HDMI output. A board with a low rate limit can also
# select a weak swing, which is too weak for the platform maximum. With
# --level-shift, the copy also gets another entry. docs/hardware.md
# explains when a copy helps and how the kernel loads it.
import argparse
import sys

# The values of the field hdmi_max_data_rate in MHz. 0 is the platform maximum.
RATES = {0: "platform maximum", 1: "297 MHz", 2: "165 MHz", 3: "594 MHz",
         4: "340 MHz", 5: "300 MHz"}
DEVICE_TYPE_TMDS = 1 << 4
DEVICE_TYPE_NOT_HDMI = 1 << 11
BLOCK_GENERAL_DEFINITIONS = 2
# hdmi_max_data_rate is in bits 5 to 7 of byte 7 of a child device, from
# BDB version 204. hdmi_level_shifter_value is in bits 0 to 4 of the same
# byte.
RATE_BYTE = 7
RATE_SHIFT = 5
LEVEL_MASK = (1 << RATE_SHIFT) - 1
CHECKSUM_OFFSET = 26


def fail(message):
    sys.exit(f"ERROR: {message}")


def u16(data, offset):
    return int.from_bytes(data[offset:offset + 2], "little")


def hdmi_children(vbt):
    """Returns the offset of each HDMI child device in the VBT."""
    if vbt[:4] != b"$VBT":
        fail("The file is not a VBT.")
    bdb = int.from_bytes(vbt[28:32], "little")
    if vbt[bdb:bdb + 16] != b"BIOS_DATA_BLOCK ":
        fail("The VBT has no BIOS data block.")
    version = u16(vbt, bdb + 16)
    if version < 204:
        fail(f"BDB version {version} has no HDMI rate field.")
    end = bdb + u16(vbt, bdb + 20)
    block = bdb + u16(vbt, bdb + 18)
    while block + 3 <= end:
        block_id, size = vbt[block], u16(vbt, block + 1)
        data = block + 3
        if block_id == BLOCK_GENERAL_DEFINITIONS:
            child_size = vbt[data + 4]
            children = []
            for child in range(data + 5, data + size - child_size + 1, child_size):
                device_type = u16(vbt, child + 2)
                if device_type & DEVICE_TYPE_TMDS and not device_type & DEVICE_TYPE_NOT_HDMI:
                    children.append(child)
            return children
        block = data + size
    fail("The VBT has no child devices.")


def arguments():
    parser = argparse.ArgumentParser(prog="kernel/vbt-hdmi-rate.py")
    parser.add_argument("vbt")
    parser.add_argument("output", nargs="?")
    parser.add_argument("--level-shift", type=int, metavar="ENTRY",
                        help="the entry of the swing table for the copy")
    args = parser.parse_args()
    if args.level_shift is not None:
        if not args.output:
            fail("--level-shift needs an OUTPUT file.")
        if not 0 <= args.level_shift <= LEVEL_MASK:
            fail(f"The level shift is a number from 0 to {LEVEL_MASK}.")
    return args


def main():
    args = arguments()
    with open(args.vbt, "rb") as f:
        vbt = bytearray(f.read())
    children = hdmi_children(vbt)
    if not children:
        fail("The VBT has no HDMI port.")
    for child in children:
        rate = vbt[child + RATE_BYTE] >> RATE_SHIFT
        level = vbt[child + RATE_BYTE] & LEVEL_MASK
        print(f"HDMI port, DVO port {vbt[child + 16]:#04x}: "
              f"{RATES.get(rate, f'unknown value {rate}')}, level shift {level}")
    if not args.output:
        return
    for child in children:
        old = vbt[child + RATE_BYTE]
        level = old & LEVEL_MASK if args.level_shift is None else args.level_shift
        vbt[child + RATE_BYTE] = level
        # Keep the byte sum of the table, so that the checksum stays valid.
        vbt[CHECKSUM_OFFSET] = (vbt[CHECKSUM_OFFSET] + old - vbt[child + RATE_BYTE]) & 0xFF
    with open(args.output, "wb") as f:
        f.write(vbt)
    print(f"Wrote {args.output} with the platform maximum for each HDMI port"
          + ("." if args.level_shift is None else f" and level shift {args.level_shift}."))


if __name__ == "__main__":
    main()
