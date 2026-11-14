#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Exit 0 if jellyfin-mpv-shim has a server login, else 1.

    has-login.py USERS_JSON

The shim keeps its profiles in users.json. A login is a credential with an
access token in any profile.
"""
import json
import sys


def has_login(path):
    try:
        with open(path, encoding="utf-8") as file:
            profiles = json.load(file).get("users", [])
    except (OSError, ValueError):
        return False
    return any(credential.get("AccessToken")
               for profile in profiles
               for credential in profile.get("credentials", []))


if __name__ == "__main__":
    sys.exit(0 if has_login(sys.argv[1]) else 1)
