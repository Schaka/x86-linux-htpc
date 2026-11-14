#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Set keys in the configuration file of the HTPC.

    set-config.py FILE EXAMPLE KEY=VALUE...

If FILE does not exist, the script starts from EXAMPLE, so the new file has
all comments of the example. A key that the file has gets its new value in
its own line. A new key goes to the end. All other lines stay.

The library reads the file as data and removes one pair of double quotes,
so a value with a space gets double quotes here. A value cannot contain a
double quote or a line break.
"""
import os
import re
import sys
import tempfile


def quoted(value):
    if value == "" or re.fullmatch(r"[A-Za-z0-9_./:@,+-]+", value):
        return value
    return f'"{value}"'


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    path, example = sys.argv[1], sys.argv[2]
    pairs = []
    for arg in sys.argv[3:]:
        key, sep, value = arg.partition("=")
        if not sep or not re.fullmatch(r"[A-Z][A-Z0-9_]*", key):
            sys.exit(f"ERROR: {arg} is not KEY=VALUE.")
        if '"' in value or "\n" in value:
            sys.exit(f"ERROR: the value of {key} contains a double quote or a line break.")
        pairs.append((key, value))

    source = path if os.path.exists(path) else example
    with open(source) as f:
        lines = f.read().splitlines()

    for key, value in pairs:
        line = f"{key}={quoted(value)}"
        pattern = re.compile(rf"^\s*{key}=")
        matches = [i for i, text in enumerate(lines) if pattern.match(text)]
        if matches:
            # The library reads the last line of a key, so that line changes.
            lines[matches[-1]] = line
        else:
            lines.append(line)

    directory = os.path.dirname(os.path.abspath(path))
    os.makedirs(directory, exist_ok=True)
    handle, partial = tempfile.mkstemp(dir=directory, prefix=".htpc.conf.")
    with os.fdopen(handle, "w") as f:
        f.write("\n".join(lines) + "\n")
    os.chmod(partial, 0o644)
    os.replace(partial, path)


if __name__ == "__main__":
    main()
