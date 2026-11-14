#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Installs jellyfin-mpv-shim for the HTPC user with pipx and starts it with
# the Plasma session. pipx and not the Flatpak: the Flatpak brings its own
# mpv, and the shim must be able to start the CB1 mpv.
#
# JELLYFIN_MPV in the configuration file selects the mpv. "system" plays in
# Plasma with the libmpv of the distribution. "cb1" plays all videos with
# the CB1 mpv on its own VT. docs/players.md explains the two.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
shim_version="3.3.0"
shim_conf_dir=".config/jellyfin-mpv-shim"
desktop_id=com.github.iwalton3.jellyfin-mpv-shim
cb1_mpv=/opt/x86-linux-htpc/cb1/bin/mpv
# logind fills VTs 1 to 6 with a login prompt on demand. The CB1 mpv takes
# a VT above them.
cb1_vt=8
cb1_scripts=(htpc-output.lua htpc-screen.run)
# The script of the mpv of the distribution
system_scripts=(htpc-refresh.lua)

mpv_mode() {
	local mode
	mode="$(config_get JELLYFIN_MPV system)"
	case "${mode}" in
	system | cb1) printf '%s\n' "${mode}" ;;
	*) fail "JELLYFIN_MPV must be system or cb1, not ${mode}." ;;
	esac
}

# The environment sees the system site packages, because PyGObject for the
# MPRIS media control comes from the distribution and not from PyPI.
install_shim() {
	local installed venv_config
	installed="$(run_as_htpc_user pipx list --short 2>/dev/null | awk '$1 == "jellyfin-mpv-shim" {print $2}')"
	venv_config="$(htpc_user_home)/.local/share/pipx/venvs/jellyfin-mpv-shim/pyvenv.cfg"
	if [[ "${installed}" != "${shim_version}" ]] ||
		! grep -qx 'include-system-site-packages = true' "${venv_config}" 2>/dev/null; then
		run_as_htpc_user pipx install --force --system-site-packages "jellyfin-mpv-shim==${shim_version}"
	fi
}

# mpv takes the formats as one comma list, for example "ac3,eac3,truehd".
audio_spdif_value() {
	local formats
	formats="$(config_get AUDIO_PASSTHROUGH_FORMATS "ac3 eac3 truehd dts dts-hd")"
	tr -s ' ' ',' <<<"${formats}"
}

# The size of the display mode, for example 3840x2160 from 3840x2160@60
display_resolution() {
	local mode
	mode="$(config_get DISPLAY_MODE)"
	printf '%s\n' "${mode%@*}"
}

match_rate() {
	local value
	value="$(config_get DISPLAY_MATCH_RATE yes)"
	case "${value}" in
	yes | no) printf '%s\n' "${value}" ;;
	*) fail "DISPLAY_MATCH_RATE must be yes or no, not ${value}." ;;
	esac
}

# The size for the display modes of the CB1 mpv. Without a size, the CB1
# mpv keeps the preferred mode of the display for each video.
cb1_resolution() {
	[[ "$(match_rate)" == yes ]] && display_resolution
	return 0
}

install_mpv_options() {
	local generated template=mpv-options.conf.in
	[[ "$(mpv_mode)" == cb1 ]] && template=mpv-options-cb1.conf.in
	generated="$(mktemp)"
	sed -e "s/@AUDIO_SPDIF@/$(audio_spdif_value)/" \
		-e "s/@VT@/${cb1_vt}/" \
		-e "s/@RESOLUTION@/$(cb1_resolution)/" \
		-e "s/@OUTPUT@/$(config_get HDMI_OUTPUT)/" \
		"${component_dir}/files/${template}" >"${generated}"
	install_user_file "${generated}" "${shim_conf_dir}/x86-linux-htpc.conf"
	rm -f "${generated}"

	# "~~/" is the configuration folder of mpv. The shim sets it to its own
	# folder, so this line finds the file above.
	local include_line='include="~~/x86-linux-htpc.conf"'
	local mpv_conf
	mpv_conf="$(htpc_user_home)/${shim_conf_dir}/mpv.conf"
	if ! grep -qxF "${include_line}" "${mpv_conf}" 2>/dev/null; then
		local with_include
		with_include="$(mktemp)"
		{
			printf '%s\n' "${include_line}"
			cat "${mpv_conf}" 2>/dev/null || true
		} >"${with_include}"
		install_user_file "${with_include}" "${shim_conf_dir}/mpv.conf"
		rm -f "${with_include}"
	fi
}

# The CB1 mpv runs as its own process, which the shim controls over IPC.
# Its two scripts switch the VT and set the output for each video.
install_cb1_scripts() {
	local script mode
	for script in "${cb1_scripts[@]}"; do
		mode=0644
		[[ "${script}" == *.run ]] && mode=0755
		install_user_file "${component_dir}/files/scripts/${script}" "${shim_conf_dir}/scripts/${script}" "${mode}"
	done
}

remove_scripts() {
	local script
	for script in "$@"; do
		remove_file "$(htpc_user_home)/${shim_conf_dir}/scripts/${script}"
	done
}

# Inside Plasma, the script htpc-refresh.lua gives the display the refresh
# rate of each video.
install_system_scripts() {
	local script
	if [[ "$(match_rate)" == no ]]; then
		remove_scripts "${system_scripts[@]}"
		return 0
	fi
	for script in "${system_scripts[@]}"; do
		install_user_file "${component_dir}/files/scripts/${script}" "${shim_conf_dir}/scripts/${script}"
	done
}

# On its own VT, the CB1 mpv needs the DRM card and the input devices, and
# PipeWire needs the sound card. logind gives these rights only to the
# session in front, and without them PipeWire removes the HDMI sink. So the
# user needs the groups. A new group applies at the next login.
add_cb1_groups() {
	local group
	for group in video input audio; do
		if ! id -nG "$(htpc_user)" | tr ' ' '\n' | grep -qx "${group}"; then
			usermod -aG "${group}" "$(htpc_user)"
			info "Added $(htpc_user) to the group ${group}. Log in again to apply it."
		fi
	done
}

# The launcher entry "Jellyfin" opens the library browser of the shim. The
# shim runs as a service, so a second start only asks the running shim to
# show its window. The entry has the desktop ID of the shim, so that Plasma
# shows the icon of the entry for the mpv window.
install_launcher_entry() {
	local home integration size generated
	home="$(htpc_user_home)"
	integration="$(compgen -G "${home}/.local/share/pipx/venvs/jellyfin-mpv-shim/lib/python3*/site-packages/jellyfin_mpv_shim/integration" | head -n 1)"
	[[ -n "${integration}" ]] || fail "The shim has no integration folder with its icons."
	for size in 16 32 48 64 128 256; do
		install_user_file "${integration}/jellyfin-${size}.png" \
			".local/share/icons/hicolor/${size}x${size}/apps/${desktop_id}.png"
	done
	generated="$(mktemp)"
	cat >"${generated}" <<ENTRY
[Desktop Entry]
Type=Application
Name=Jellyfin
Comment=Jellyfin library and player
Exec=${home}/.local/bin/jellyfin-mpv-shim
Icon=${desktop_id}
Categories=AudioVideo;Video;Player;TV;
StartupWMClass=${desktop_id}
ENTRY
	install_user_file "${generated}" ".local/share/applications/${desktop_id}.desktop"
	rm -f "${generated}"
}

install_shim_options() {
	local -a options=(
		fullscreen=true
		browser_fullscreen=true
		# The library browser of the shim is the Jellyfin app on the TV. The
		# launcher entry opens it. Plasma Bigscreen has no system tray, so the
		# shim waits in the background until the entry or a cast opens mpv.
		enable_gui=true
		start_minimized=true
		allow_background=true
		# install.sh pins the version, so the shim must not ask for updates.
		check_updates=false
		# The shim sets hwdec for each file, and its default is "no". "auto"
		# picks VA-API for the GPU outputs of both mpv builds.
		hwdec='"auto"'
	)
	# The browser follows the scale of the compositor. The DRM output of the
	# CB1 mpv and a TV session in Plasma often have scale 1, so the browser
	# scales with the display width, from a base of 1920 pixels.
	local width
	width="$(display_resolution)"
	width="${width%%x*}"
	if [[ "${width}" =~ ^[0-9]+$ ]]; then
		options+=("ui_scale=$(python3 -c 'import sys; print(round(int(sys.argv[1]) / 1920, 2))' "${width}")")
	fi
	if [[ "$(mpv_mode)" == cb1 ]]; then
		options+=(mpv_ext=true "mpv_ext_path=\"${cb1_mpv}\"" mpv_ext_start=true)
	else
		options+=(mpv_ext=false mpv_ext_path=null)
	fi
	local player_name
	player_name="$(config_get JELLYFIN_PLAYER_NAME)"
	if [[ -n "${player_name}" ]]; then
		options+=("player_name=$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "${player_name}")")
	fi
	merge_user_json "${shim_conf_dir}/conf.json" "${options[@]}"
}

install_packages "${component_dir}"
install_shim
if [[ "$(mpv_mode)" == cb1 ]]; then
	[[ -x "${cb1_mpv}" ]] || fail "JELLYFIN_MPV=cb1 needs ${cb1_mpv}. Build it with cb1/build-in-container.sh."
	add_cb1_groups
	install_cb1_scripts
	remove_scripts "${system_scripts[@]}"
else
	remove_scripts "${cb1_scripts[@]}"
	install_system_scripts
fi
install_mpv_options
install_shim_options
install_launcher_entry
enable_user_service "${component_dir}/files/jellyfin-mpv-shim.service"

# A running session gets the new options at once. Without a session, the
# service starts with the next login.
if run_as_htpc_user systemctl --user is-active --quiet graphical-session.target 2>/dev/null; then
	run_as_htpc_user systemctl --user restart jellyfin-mpv-shim.service
fi
