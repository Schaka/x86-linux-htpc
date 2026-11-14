#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Copy the app icon out of an APK.

    apk-icon.py APK OUTPUT_BASE

Writes OUTPUT_BASE.png or OUTPUT_BASE.webp and prints its path. If the APK
has no icon as a picture, the exit code is 1.

Waydroid exports icons only for apps with a phone launcher. TV apps have
only a TV launcher, so their tile on the home screen would have no icon.
The APK has the icon, but release builds shorten the file names, for
example res/d2.webp. So the script reads the icon resource from the binary
manifest and finds its files in the resource table. It takes the picture
with the highest density. An adaptive icon of Android 8 is an XML file,
and the same resource always has pictures for the older densities too.

The APK comes from the internet, so the script only parses it, with the
standard library, and checks each offset.
"""
import struct
import sys
import zipfile

ATTR_ICON = 0x01010002
TYPE_REFERENCE = 0x01
TYPE_STRING = 0x03
DENSITY_ANY = 0xFFFE
DENSITY_NONE = 0xFFFF


class ApkError(Exception):
    pass


def u16(data, offset):
    if offset < 0 or offset + 2 > len(data):
        raise ApkError("offset out of range")
    return struct.unpack_from("<H", data, offset)[0]


def u32(data, offset):
    if offset < 0 or offset + 4 > len(data):
        raise ApkError("offset out of range")
    return struct.unpack_from("<I", data, offset)[0]


def chunks(data, start, end):
    """The chunks between start and end: (type, header size, offset, size)"""
    offset = start
    while offset + 8 <= end:
        kind, header_size, size = u16(data, offset), u16(data, offset + 2), u32(data, offset + 4)
        if size < 8 or offset + size > end:
            raise ApkError("bad chunk size")
        yield kind, header_size, offset, size
        offset += size


def string_pool(data, offset):
    count, flags, strings_start = u32(data, offset + 8), u32(data, offset + 16), u32(data, offset + 20)
    utf8 = flags & 0x100
    header_size = u16(data, offset + 2)

    def read(index):
        if index >= count:
            return None
        position = offset + strings_start + u32(data, offset + header_size + 4 * index)
        if utf8:
            # The length in UTF-16 units, then the length in bytes, each
            # in one or two bytes.
            for _ in range(2):
                length = data[position]
                position += 1
                if length & 0x80:
                    length = ((length & 0x7F) << 8) | data[position]
                    position += 1
            return data[position:position + length].decode("utf-8", "replace")
        length = u16(data, position)
        position += 2
        if length & 0x8000:
            length = ((length & 0x7FFF) << 16) | u16(data, position)
            position += 2
        return data[position:position + 2 * length].decode("utf-16-le", "replace")

    return read


def icon_resource(manifest):
    """The resource ID of android:icon of the element application"""
    if u16(manifest, 0) != 0x0003:
        raise ApkError("the manifest is not binary XML")
    strings = None
    resource_ids = []
    for kind, header_size, offset, size in chunks(manifest, u16(manifest, 2), len(manifest)):
        if kind == 0x0001:
            strings = string_pool(manifest, offset)
        elif kind == 0x0180:
            resource_ids = [u32(manifest, offset + header_size + 4 * i)
                            for i in range((size - header_size) // 4)]
        elif kind == 0x0102 and strings:
            if strings(u32(manifest, offset + 20)) != "application":
                continue
            attribute_start = offset + 16 + u16(manifest, offset + 24)
            attribute_size = u16(manifest, offset + 26)
            for i in range(u16(manifest, offset + 28)):
                attribute = attribute_start + i * attribute_size
                name = u32(manifest, attribute + 4)
                # The name can be shortened. The resource map gives the
                # attribute by its ID.
                if name < len(resource_ids) and resource_ids[name] == ATTR_ICON:
                    if manifest[attribute + 15] == TYPE_REFERENCE:
                        return u32(manifest, attribute + 16)
            return None
    return None


def resource_files(table, resource_id):
    """The file names of a resource, each with the density of its configuration"""
    package_id, type_id, entry_id = resource_id >> 24, (resource_id >> 16) & 0xFF, resource_id & 0xFFFF
    if u16(table, 0) != 0x0002:
        raise ApkError("resources.arsc is not a resource table")
    values = None
    files = []
    for kind, _, offset, size in chunks(table, u16(table, 2), len(table)):
        if kind == 0x0001:
            values = string_pool(table, offset)
        elif kind == 0x0200 and values and u32(table, offset + 8) == package_id:
            files += package_files(table, offset, size, type_id, entry_id, values)
    return files


def package_files(table, offset, size, type_id, entry_id, values):
    files = []
    for kind, header_size, chunk, _ in chunks(table, offset + u16(table, offset + 2), offset + size):
        if kind != 0x0201 or table[chunk + 8] != type_id:
            continue
        flags = table[chunk + 9]
        count, entries_start = u32(table, chunk + 12), u32(table, chunk + 16)
        density = u16(table, chunk + 20 + 14)
        entry = entry_offset(table, chunk + header_size, flags, count, entry_id)
        if entry is None:
            continue
        entry += chunk + entries_start
        entry_flags = u16(table, entry + 2)
        if entry_flags & 0x0008:
            # A compact entry keeps the type of the value in its flags.
            data_type, data = entry_flags >> 8, u32(table, entry + 4)
        elif entry_flags & 0x0001:
            continue
        else:
            value = entry + u16(table, entry)
            data_type, data = table[value + 3], u32(table, value + 4)
        if data_type == TYPE_STRING:
            name = values(data)
            if name:
                files.append((density, name))
    return files


def entry_offset(table, offsets, flags, count, entry_id):
    """The offset of an entry from the start of the entries, or None"""
    if flags & 0x01:
        # Sparse: pairs of entry ID and offset / 4, sorted by the ID.
        for i in range(count):
            if u16(table, offsets + 4 * i) == entry_id:
                return u16(table, offsets + 4 * i + 2) * 4
        return None
    if entry_id >= count:
        return None
    if flags & 0x02:
        value = u16(table, offsets + 2 * entry_id)
        return None if value == 0xFFFF else value * 4
    value = u32(table, offsets + 4 * entry_id)
    return None if value == 0xFFFFFFFF else value


def rank(density):
    """Real densities first, the largest first. A picture without density
    comes after them."""
    if density in (DENSITY_ANY, 0):
        return 1
    if density == DENSITY_NONE:
        return 2
    return 3 + density


def picture_format(content):
    if content.startswith(b"\x89PNG\r\n\x1a\n"):
        return ".png"
    if content[:4] == b"RIFF" and content[8:12] == b"WEBP":
        return ".webp"
    return None


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    apk_path, output_base = sys.argv[1], sys.argv[2]
    try:
        with zipfile.ZipFile(apk_path) as apk:
            resource_id = icon_resource(apk.read("AndroidManifest.xml"))
            if resource_id is None:
                sys.exit("The APK has no icon.")
            files = resource_files(apk.read("resources.arsc"), resource_id)
            # Some builds keep the files without an extension, so the
            # first bytes tell the format.
            for _, name in sorted(((rank(d), n) for d, n in files), reverse=True):
                if name.endswith(".xml") or name not in apk.namelist():
                    continue
                content = apk.read(name)
                extension = picture_format(content)
                if extension:
                    break
            else:
                sys.exit("The icon of the APK has no picture.")
            output = output_base + extension
            with open(output, "wb") as f:
                f.write(content)
    except (ApkError, KeyError, zipfile.BadZipFile, struct.error, IndexError) as error:
        sys.exit(f"Cannot read the icon of {apk_path}: {error}")
    print(output)


if __name__ == "__main__":
    main()
