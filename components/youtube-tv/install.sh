#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Installs VacuumTube, the TV interface of YouTube for Linux, from Flathub,
# with a launcher that gives the TV the refresh rate and the HDR mode of
# each video. docs/players.md explains the launcher.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
app_id=rocks.shy.VacuumTube
launcher=/usr/local/libexec/x86-linux-htpc/youtube-tv
app_config=".var/app/${app_id}/config/VacuumTube"
# The launcher reads the player through this port on localhost.
debug_port=47811

install_app() {
	flatpak remotes --columns=name | grep -qx flathub ||
		flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
	flatpak info "${app_id}" >/dev/null 2>&1 ||
		flatpak install -y --noninteractive flathub "${app_id}"
}

# wayland_hdr lets Chromium describe HDR video to KWin. Without it, the app
# turns the color protocol off and plays SDR only. fullscreen fits a TV.
# unlock_resolution allows 4K when Plasma scales the desktop.
configure_app() {
	local generated
	merge_user_json "${app_config}/config.json" wayland_hdr=true fullscreen=true unlock_resolution=true
	generated="$(mktemp)"
	printf '%s\n' "--remote-debugging-port=${debug_port}" >"${generated}"
	install_user_file "${generated}" "${app_config}/flags.txt"
	rm -f "${generated}"
}

install_desktop_entry() {
	local generated
	generated="$(mktemp)"
	cat >"${generated}" <<ENTRY
[Desktop Entry]
Type=Application
Name=YouTube
Comment=VacuumTube with the refresh rate and HDR of each video
Exec=${launcher} %U
Icon=${app_id}
Categories=AudioVideo;Video;TV;
ENTRY
	install_user_file "${generated}" ".local/share/applications/htpc-youtube-tv.desktop"
	rm -f "${generated}"
}

install_packages "${component_dir}"
install_app
configure_app
install_file "${component_dir}/files/youtube-tv" "${launcher}" 0755
install_desktop_entry
