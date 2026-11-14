#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Download the newest APK of an app from its GitHub releases.

    fetch-apk.py GITHUB_REPOSITORY ASSET_PATTERN DIRECTORY

Prints the path of the APK. GitHub lists a SHA256 digest for each release
asset, and the script compares the download with it. A file that is
already in DIRECTORY with the right digest is not downloaded again.
"""
import hashlib
import json
import os
import re
import sys
import urllib.request


def sha256_of(path):
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def newest_asset(repository, pattern):
    url = f"https://api.github.com/repos/{repository}/releases?per_page=20"
    with urllib.request.urlopen(url, timeout=60) as response:
        releases = json.load(response)
    releases.sort(key=lambda release: release["published_at"], reverse=True)
    for release in releases:
        # One release can hold several builds. The newest upload wins.
        assets = [asset for asset in release["assets"] if re.search(pattern, asset["name"])]
        if assets:
            return max(assets, key=lambda asset: asset["created_at"])
    sys.exit(f"ERROR: {repository} has no release asset that matches {pattern}.")


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    repository, pattern, directory = sys.argv[1:]
    asset = newest_asset(repository, pattern)
    algorithm, _, expected = (asset.get("digest") or "").partition(":")
    if algorithm != "sha256":
        sys.exit(f"ERROR: GitHub lists no SHA256 digest for {asset['name']}.")
    path = os.path.join(directory, asset["name"])
    if not (os.path.isfile(path) and sha256_of(path) == expected):
        print(f"== Downloading {asset['name']}", file=sys.stderr)
        partial = path + ".part"
        urllib.request.urlretrieve(asset["browser_download_url"], partial)
        if sha256_of(partial) != expected:
            os.remove(partial)
            sys.exit(f"ERROR: The download of {asset['name']} has the wrong SHA256 sum.")
        os.replace(partial, path)
    print(path)


if __name__ == "__main__":
    main()
