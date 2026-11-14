#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

prefix=/opt/x86-linux-htpc/kodi

kodi_runs() {
	"${prefix}/bin/kodi" --version 2>/dev/null | grep -q "^Kodi"
}

libraries_found() {
	! ldd "${prefix}/lib64/kodi/kodi-gbm" 2>/dev/null | grep -q "not found"
}

uses_cb1_libraries() {
	ldd "${prefix}/lib64/kodi/kodi-gbm" 2>/dev/null | grep -q "libplacebo.* => /opt/x86-linux-htpc/cb1/lib/"
}

launcher_installed() {
	[[ -x /usr/local/libexec/x86-linux-htpc/kodi-dv ]] &&
		[[ -f "$(htpc_user_home)/.local/share/applications/htpc-kodi.desktop" ]]
}

user_groups() {
	local group
	for group in video input audio; do
		id -nG "$(htpc_user)" | tr ' ' '\n' | grep -qx "${group}" || return 1
	done
}

check "Kodi starts" kodi_runs
check "Kodi finds all its libraries" libraries_found
check "Kodi uses the FFmpeg and libplacebo of the CB1 build" uses_cb1_libraries
check "The launcher and the tile are installed" launcher_installed
check "The HTPC user is in the groups video, input and audio" user_groups

checks_done
