#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Stand-in for sha512hmac of libkcapi, for build containers.

kernel.spec writes the FIPS checksum of the kernel image with sha512hmac.
That tool computes the HMAC through the crypto API of the running kernel
(AF_ALG). Inside a container, the kernel of the host can lack hmac(sha512)
there, and the build stops. This script writes the same output in user
space: the HMAC-SHA512 with the fixed key of hmaccalc, two spaces, the name.

    sha512hmac FILE...
"""
import hashlib
import hmac
import sys

HMACCALC_KEY = b"FIPS-FTW-RHT2009"


def main():
    for name in sys.argv[1:]:
        with open(name, "rb") as file:
            digest = hmac.new(HMACCALC_KEY, file.read(), hashlib.sha512).hexdigest()
        print(f"{digest}  {name}")


if __name__ == "__main__":
    main()
