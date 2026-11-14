#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Installs Kodi with the CB1 engine from the archive of
# cb1/build-in-container.sh, and a tile that starts it on its own VT.
# docs/players.md describes the design.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
prefix=/opt/x86-linux-htpc/kodi
launcher=/usr/local/libexec/x86-linux-htpc/kodi-dv

# The archive holds the folder /opt/x86-linux-htpc/kodi. The SHA256 sum of
# the unpacked archive stays in the folder, so the same archive is not
# unpacked twice.
install_kodi() {
	local build_dir archive sum
	build_dir="$(config_require CB1_BUILD_DIR)"
	archive="${build_dir}/x86-linux-htpc-kodi-fc$(fedora_release).tar.gz"
	[[ -f "${archive}" ]] ||
		fail "${archive} is missing. Build it with cb1/build-in-container.sh $(fedora_release)."
	sum="$(sha256sum "${archive}" | cut -d' ' -f1)"
	if [[ "$(cat "${prefix}/.archive-sha256" 2>/dev/null)" == "${sum}" ]]; then
		return 0
	fi
	rm -rf "${prefix}"
	tar -xzf "${archive}" -C / opt/x86-linux-htpc/kodi
	printf '%s\n' "${sum}" >"${prefix}/.archive-sha256"
	info "Installed Kodi from $(basename "${archive}")"
}

# On its own VT, Kodi has no login session. It needs the DRM card and the
# input devices, and PipeWire needs the sound card. A new group applies at
# the next login.
add_groups() {
	local group
	for group in video input audio; do
		if ! id -nG "$(htpc_user)" | tr ' ' '\n' | grep -qx "${group}"; then
			usermod -aG "${group}" "$(htpc_user)"
			info "Added $(htpc_user) to the group ${group}. Log in again to apply it."
		fi
	done
}

# Kodi reads the defaults of its settings from appliance.xml. A setting that
# the user changed in Kodi keeps the value of the user. The passthrough
# follows AUDIO_PASSTHROUGH_FORMATS, as in the other players. With TrueHD,
# a receiver decodes the sound, so decoded sound goes out as 7.1. The menu
# sounds stay off, because they play over the receiver at full volume.
install_settings_defaults() {
	local formats generated format value channels=1 passthrough=false
	formats=" $(config_get AUDIO_PASSTHROUGH_FORMATS "ac3 eac3 truehd dts dts-hd") "
	[[ "${formats}" == *" truehd "* ]] && channels=10
	[[ "${formats}" != "  " ]] && passthrough=true
	generated="$(mktemp)"
	# Kodi finds each setting by its section, category and group in
	# system/settings/settings.xml.
	{
		printf '%s\n' '<?xml version="1.0" encoding="utf-8" ?>' \
			'<!-- Written by the component kodi-dv of x86-linux-htpc. -->' \
			'<settings version="1">' '  <section id="system">' '    <category id="audio">'
		printf '      <group id="1"><setting id="audiooutput.channels"><default>%s</default></setting></group>\n' "${channels}"
		printf '      <group id="2"><setting id="audiooutput.guisoundmode"><default>0</default></setting></group>\n'
		printf '      <group id="3">\n'
		printf '        <setting id="audiooutput.passthrough"><default>%s</default></setting>\n' "${passthrough}"
		for format in ac3 eac3 dts truehd dts-hd; do
			value=false
			[[ "${formats}" == *" ${format} "* ]] && value=true
			printf '        <setting id="audiooutput.%spassthrough"><default>%s</default></setting>\n' "${format//-/}" "${value}"
		done
		printf '%s\n' '      </group>' '    </category>' '  </section>' '</settings>'
	} >"${generated}"
	install_file "${generated}" "${prefix}/share/kodi/system/settings/appliance.xml" 0644
	rm -f "${generated}"
}

install_desktop_entry() {
	local generated
	generated="$(mktemp)"
	cat >"${generated}" <<ENTRY
[Desktop Entry]
Type=Application
Name=Kodi
Comment=Kodi with Dolby Vision on its own screen
Exec=${launcher}
Icon=${prefix}/share/icons/hicolor/256x256/apps/kodi.png
Categories=AudioVideo;Video;TV;
ENTRY
	install_user_file "${generated}" ".local/share/applications/htpc-kodi.desktop"
	rm -f "${generated}"
}

install_packages "${component_dir}"
install_kodi
install_settings_defaults
add_groups
install_file "${component_dir}/files/kodi-dv" "${launcher}" 0755
install_desktop_entry
install_user_file "${component_dir}/files/htpc-remote.xml" ".kodi/userdata/keymaps/htpc-remote.xml"
