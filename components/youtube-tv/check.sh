#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

app_config="$(htpc_user_home)/.var/app/rocks.shy.VacuumTube/config/VacuumTube"

app_installed() {
	flatpak info rocks.shy.VacuumTube >/dev/null 2>&1
}

hdr_option_on() {
	python3 -c 'import json, sys; sys.exit(0 if json.load(open(sys.argv[1])).get("wayland_hdr") else 1)' \
		"${app_config}/config.json" 2>/dev/null
}

debug_port_set() {
	grep -q -- "--remote-debugging-port=47811" "${app_config}/flags.txt" 2>/dev/null
}

launcher_installed() {
	[[ -x /usr/local/libexec/x86-linux-htpc/youtube-tv ]] &&
		[[ -f "$(htpc_user_home)/.local/share/applications/htpc-youtube-tv.desktop" ]]
}

check "VacuumTube is installed" app_installed
check "VacuumTube describes HDR video to KWin" hdr_option_on
check "The launcher can read the player of VacuumTube" debug_port_set
check "The launcher and its desktop entry are installed" launcher_installed

checks_done
