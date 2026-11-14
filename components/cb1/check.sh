#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

prefix=/opt/x86-linux-htpc/cb1

mpv_runs() {
	"${prefix}/bin/mpv" --version 2>/dev/null | grep -q "^mpv"
}

libraries_found() {
	! ldd "${prefix}/bin/mpv" 2>/dev/null | grep -q "not found"
}

uses_own_libraries() {
	ldd "${prefix}/bin/mpv" 2>/dev/null | grep -q "libplacebo.* => ${prefix}/lib/"
}

check "The CB1 mpv starts" mpv_runs
check "The CB1 mpv finds all its libraries" libraries_found
check "The CB1 mpv uses the patched libplacebo" uses_own_libraries

checks_done
