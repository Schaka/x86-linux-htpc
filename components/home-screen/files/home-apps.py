#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Write the Favorites row of the Plasma Bigscreen home screen.

    home-apps.py FAVORITES_FILE DESKTOP_ID...

Bigscreen keeps its favorites in the KConfig file "bigscreen-favs". Each
favorite is a group [Favs][N] with the fields of the desktop entry of the
app. The home screen starts the app by its desktop ID and shows the name and
the icon from these fields. A desktop ID without a desktop entry is left out
with a warning, because its tile cannot start anything. The file is written
only if its content changes.
"""
import configparser
import os
import sys

APPLICATION_DIRS = (
    os.path.expanduser("~/.local/share/applications"),
    os.path.expanduser("~/.local/share/flatpak/exports/share/applications"),
    "/var/lib/flatpak/exports/share/applications",
    "/usr/local/share/applications",
    "/usr/share/applications",
)


def find_entry(desktop_id):
    for folder in APPLICATION_DIRS:
        path = os.path.join(folder, desktop_id)
        if os.path.isfile(path):
            return path
    return None


def read_entry(path):
    parser = configparser.ConfigParser(interpolation=None, strict=False)
    parser.optionxform = str
    parser.read(path, encoding="utf-8")
    return parser["Desktop Entry"] if parser.has_section("Desktop Entry") else {}


def kconfig_value(value):
    # KConfig reads a backslash as the start of an escape and drops
    # leading spaces.
    return value.replace("\\", "\\\\").strip()


def favorite_group(number, desktop_id, path, entry):
    categories = [c for c in entry.get("Categories", "").split(";") if c]
    fields = {
        "categories": ",".join(categories),
        "comment": entry.get("Comment", ""),
        "desktopPath": path,
        "entryPath": entry.get("Exec", ""),
        "icon": entry.get("Icon", ""),
        "name": entry.get("Name", desktop_id),
        "startupNotify": "true" if entry.get("StartupNotify", "").lower() == "true" else "false",
        "storageId": desktop_id,
    }
    lines = [f"[Favs][{number}]"]
    lines += [f"{key}={kconfig_value(value)}" for key, value in fields.items()]
    return "\n".join(lines) + "\n"


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    target, desktop_ids = sys.argv[1], sys.argv[2:]
    groups = []
    for desktop_id in desktop_ids:
        path = find_entry(desktop_id)
        if path is None:
            print(f"WARNING: {desktop_id} has no desktop entry. Its tile is left out.", file=sys.stderr)
            continue
        groups.append(favorite_group(len(groups), desktop_id, path, read_entry(path)))
    content = "\n".join(groups)
    try:
        with open(target, encoding="utf-8") as f:
            if f.read() == content:
                return
    except FileNotFoundError:
        pass
    with open(target, "w", encoding="utf-8") as f:
        f.write(content)
    print(f"== Wrote {target} with {len(groups)} apps")


if __name__ == "__main__":
    main()
