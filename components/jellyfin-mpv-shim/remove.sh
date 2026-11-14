#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Removes jellyfin-mpv-shim and the files of x86-linux-htpc. The login and
# the own options of the user in mpv.conf and conf.json stay.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

home="$(htpc_user_home)"
run_as_htpc_user systemctl --user stop jellyfin-mpv-shim.service 2>/dev/null || true
disable_user_service jellyfin-mpv-shim.service
run_as_htpc_user pipx uninstall jellyfin-mpv-shim 2>/dev/null || true
remove_file "${home}/.config/jellyfin-mpv-shim/x86-linux-htpc.conf"
remove_file "${home}/.local/share/applications/com.github.iwalton3.jellyfin-mpv-shim.desktop"
find "${home}/.local/share/icons/hicolor" -name com.github.iwalton3.jellyfin-mpv-shim.png -delete 2>/dev/null || true
for script in htpc-output.lua htpc-screen.run htpc-refresh.lua; do
	remove_file "${home}/.config/jellyfin-mpv-shim/scripts/${script}"
done
if [[ -f "${home}/.config/jellyfin-mpv-shim/mpv.conf" ]]; then
	runuser -u "$(htpc_user)" -- sed -i '/^include="~~\/x86-linux-htpc.conf"$/d' "${home}/.config/jellyfin-mpv-shim/mpv.conf"
fi
