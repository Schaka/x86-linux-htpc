#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Installs Plasma Bigscreen, logs the HTPC user in to it automatically and
# turns off the screen lock and the automatic suspend of that user.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
session_name="plasma-bigscreen-wayland"

# Plasma Login Manager is based on SDDM and reads the same [Autologin]
# keys, each from its own drop-in folder.
autologin_dropin() {
	case "$(systemctl show -p Id --value display-manager.service)" in
	plasmalogin.service) printf '%s\n' /etc/plasmalogin.conf.d/x86-linux-htpc.conf ;;
	sddm.service) printf '%s\n' /etc/sddm.conf.d/x86-linux-htpc.conf ;;
	*) fail "The display manager is not Plasma Login Manager or SDDM. See docs/desktop.md." ;;
	esac
}

install_autologin() {
	local generated
	generated="$(mktemp)"
	cat >"${generated}" <<CONF
# Written by the component bigscreen of x86-linux-htpc.
[Autologin]
User=$(htpc_user)
Session=${session_name}
Relogin=true
CONF
	install_file "${generated}" "$(autologin_dropin)"
	rm -f "${generated}"
}

# The HTPC shows no personal data, and a lock screen or a suspend would stop
# casts from Jellyfin. Suspend can come back through the power policy. The
# automatic login has no password to unlock KWallet, so each program that
# asks for the wallet would show a password dialog on the TV.
configure_user_session() {
	run_as_htpc_user kwriteconfig6 --file kwalletrc --group Wallet --key Enabled false
	run_as_htpc_user kwriteconfig6 --file kscreenlockerrc --group Daemon --key Autolock false
	run_as_htpc_user kwriteconfig6 --file kscreenlockerrc --group Daemon --key LockOnResume false
	run_as_htpc_user kwriteconfig6 --file powerdevilrc --group AC --group SuspendAndShutdown --key AutoSuspendAction 0
}

# The pointer of an air mouse often reaches a corner of the screen by
# accident. In a corner, KWin opens the overview of windows and desktops,
# which a remote cannot close. So with SCREEN_EDGES=off, no screen edge has
# an action. The value 9 means no edge.
disable_screen_edges() {
	local effect key
	case "$(config_get SCREEN_EDGES off)" in
	off) ;;
	on)
		# Without the keys, KWin uses its own defaults again.
		for effect in overview windowview; do
			for key in BorderActivate BorderActivateAll BorderActivateClass; do
				run_as_htpc_user kwriteconfig6 --file kwinrc --group "Effect-${effect}" --key "${key}" --delete
			done
		done
		for key in Top TopRight Right BottomRight Bottom BottomLeft Left TopLeft; do
			run_as_htpc_user kwriteconfig6 --file kwinrc --group ElectricBorders --key "${key}" --delete
		done
		reload_edge_effects
		return 0
		;;
	*) fail "SCREEN_EDGES must be on or off." ;;
	esac
	for effect in overview windowview; do
		for key in BorderActivate BorderActivateAll BorderActivateClass; do
			run_as_htpc_user kwriteconfig6 --file kwinrc --group "Effect-${effect}" --key "${key}" 9
		done
	done
	for key in Top TopRight Right BottomRight Bottom BottomLeft Left TopLeft; do
		run_as_htpc_user kwriteconfig6 --file kwinrc --group ElectricBorders --key "${key}" None
	done
	reload_edge_effects
}

# The general reconfigure of KWin does not reload the effects, so each
# effect gets its own call. Without a session, KWin reads the file at the
# next login.
reload_edge_effects() {
	local effect
	for effect in overview windowview; do
		run_as_htpc_user busctl --user call org.kde.KWin /Effects org.kde.kwin.Effects \
			reconfigureEffect s "${effect}" >/dev/null 2>&1 || true
	done
}

# The power key of many remotes sends the sleep key. A suspended HTPC
# takes no cast, and some boards do not wake up from a USB remote. So the
# sleep and hibernate keys do nothing: in Plasma, and in logind when Plasma
# does not run.
ignore_sleep_keys() {
	local generated key
	generated="$(mktemp)"
	cat >"${generated}" <<CONF
# Written by the component bigscreen of x86-linux-htpc.
[Login]
HandleSuspendKey=ignore
HandleHibernateKey=ignore
CONF
	install_file "${generated}" /etc/systemd/logind.conf.d/x86-linux-htpc.conf
	rm -f "${generated}"
	for key in Sleep Hibernate; do
		run_as_htpc_user busctl --user call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel \
			setForeignShortcutKeys "asa(ai)" 4 org_kde_powerdevil "${key}" "" "" 0 2>/dev/null ||
			run_as_htpc_user kwriteconfig6 --file kglobalshortcutsrc --group org_kde_powerdevil \
				--key "${key}" "none,${key},${key}"
	done
}

# KWin hands a full-screen buffer of an app straight to the display plane
# (direct scanout). With the Android TV image of Waydroid on Intel GPUs,
# the TV then shows black flicker and shifted video, which a screenshot
# does not show. KWin then composes each frame itself. The CB1 mpv plays
# on its own VT and does not use KWin.
install_kwin_environment() {
	local generated
	generated="$(mktemp)"
	cat >"${generated}" <<CONF
# Written by the component bigscreen of x86-linux-htpc.
KWIN_DRM_NO_DIRECT_SCANOUT=1
CONF
	install_user_file "${generated}" ".config/environment.d/x86-linux-htpc-kwin.conf"
	rm -f "${generated}"
}

# KWin keeps the output mode of the last session. A user service sets the
# output mode and HDR from the configuration file at each login, so a
# change in the display settings of Plasma does not stay.
install_display_service() {
	local output mode hdr arguments folder unit
	output="$(config_require HDMI_OUTPUT)"
	mode="$(config_get DISPLAY_MODE)"
	hdr="$(config_get DISPLAY_HDR no)"
	arguments=""
	if [[ -n "${mode}" ]]; then
		[[ "${mode}" =~ ^[0-9]+x[0-9]+@[0-9.]+$ ]] ||
			fail "DISPLAY_MODE must be WIDTHxHEIGHT@RATE, for example 3840x2160@60."
		arguments+=" output.${output}.mode.${mode}"
	fi
	case "${hdr}" in
	yes) arguments+=" output.${output}.hdr.enable" ;;
	no) arguments+=" output.${output}.hdr.disable" ;;
	*) fail "DISPLAY_HDR must be yes or no." ;;
	esac
	# enable_user_service takes the unit name from the file name.
	folder="$(mktemp -d)"
	unit="${folder}/htpc-display.service"
	cat >"${unit}" <<UNIT
# Written by the component bigscreen of x86-linux-htpc from htpc.conf.
[Unit]
Description=Output mode and HDR of the TV (x86-linux-htpc)
After=graphical-session.target
PartOf=graphical-session.target

[Service]
Type=oneshot
ExecStart=/usr/bin/kscreen-doctor${arguments}

[Install]
WantedBy=graphical-session.target
UNIT
	chmod a+rx "${folder}"
	enable_user_service "${unit}"
	rm -rf "${folder}"
	run_as_htpc_user systemctl --user restart htpc-display.service 2>/dev/null ||
		info "The output settings apply at the next login."
}

install_packages "${component_dir}"
[[ -f "/usr/share/wayland-sessions/${session_name}.desktop" ]] ||
	fail "The package list installed no ${session_name}.desktop."
install_autologin
configure_user_session
disable_screen_edges
ignore_sleep_keys
install_kwin_environment
install_display_service
