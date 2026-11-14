#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Set keys in a JSON object file and keep all other keys.

    merge-json.py FILE KEY=JSON_VALUE...

Programs such as jellyfin-mpv-shim write their whole configuration back to
the file. A copy of a fixed file would delete their values on each install,
so the install scripts change only the keys that they own. The file is
written only if a value changes, so a second install run changes nothing.
"""
import json
import os
import sys


def parse_assignment(argument):
    key, separator, value = argument.partition("=")
    if not separator or not key:
        sys.exit(f"Not KEY=JSON_VALUE: {argument}")
    return key, json.loads(value)


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    path = sys.argv[1]
    wanted = dict(parse_assignment(argument) for argument in sys.argv[2:])

    current = {}
    if os.path.exists(path):
        with open(path, encoding="utf-8") as file:
            current = json.load(file)

    changed = sorted(key for key, value in wanted.items()
                     if key not in current or current[key] != value)
    if not changed:
        return
    merged = {**current, **wanted}

    os.makedirs(os.path.dirname(path), exist_ok=True)
    temporary = path + ".tmp"
    with open(temporary, "w", encoding="utf-8") as file:
        json.dump(merged, file, indent=4)
        file.write("\n")
    os.replace(temporary, path)
    print(f"Changed {path}: {', '.join(changed)}")


if __name__ == "__main__":
    main()
