#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Set string values in the settings file of an Android app.

    android-prefs.py FILE NAME=VALUE...

FILE is a SharedPreferences file of the app, for example
DATA/data/PACKAGE/shared_prefs/PACKAGE_preferences.xml in the Waydroid data
folder. All other values in the file stay. If the file does not exist yet,
the script makes it. The app then adds its own defaults at its first start
and keeps these values.

The app keeps its settings in memory and writes them back at each change.
So the app must not run while this script writes the file.
"""
import os
import sys
import xml.etree.ElementTree as ET


def main():
    if len(sys.argv) < 3 or any("=" not in arg for arg in sys.argv[2:]):
        sys.exit(__doc__)
    path = sys.argv[1]
    values = dict(arg.split("=", 1) for arg in sys.argv[2:])

    # The data folder of the app belongs to the user ID of the app. The
    # files of the script must have the same owner, or the app cannot write
    # them.
    shared_prefs = os.path.dirname(path)
    app_dir = os.path.dirname(shared_prefs)
    owner = os.stat(app_dir)
    if not os.path.isdir(shared_prefs):
        os.mkdir(shared_prefs, 0o771)
        os.chown(shared_prefs, owner.st_uid, owner.st_gid)

    if os.path.isfile(path):
        tree = ET.parse(path)
        root = tree.getroot()
    else:
        root = ET.Element("map")
        tree = ET.ElementTree(root)

    changed = False
    for name, value in values.items():
        element = root.find(f"./*[@name='{name}']")
        if element is not None and element.tag == "string" and element.text == value:
            continue
        if element is not None:
            root.remove(element)
        element = ET.SubElement(root, "string", name=name)
        element.text = value
        changed = True
    if not changed:
        return

    ET.indent(tree, space="    ")
    partial = path + ".part"
    tree.write(partial, encoding="utf-8", xml_declaration=True)
    os.chmod(partial, 0o660)
    os.chown(partial, owner.st_uid, owner.st_gid)
    os.replace(partial, path)
    print(f"Changed {path}")


if __name__ == "__main__":
    main()
