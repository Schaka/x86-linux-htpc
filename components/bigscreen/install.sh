#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Installs Plasma Bigscreen, logs the HTPC user in to it automatically and
# turns off the screen lock. The HTPC stays on: no key, lid or idle time
# suspends it or shuts it down. A KWin script keeps the home screen in
# front and in focus.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
session_name="plasma-bigscreen-wayland"
kwin_script=x86-linux-htpc-home
display_mode_program=/usr/local/libexec/x86-linux-htpc/display-mode

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

# The HTPC stays on at all times. A suspended HTPC takes no cast, and some
# boards do not wake up from a USB remote. The power key of many remotes
# sends the sleep key, and the power button of the board sends the power
# key, which makes Plasma show a shutdown prompt that a remote can confirm.
# So the keys, the lid and the idle time do nothing in logind, which also
# covers the time that Plasma does not run. logind reads the file at its
# next start, and Plasma already ignores the keys until then.
ignore_power_keys_in_logind() {
	local generated
	generated="$(mktemp)"
	cat >"${generated}" <<CONF
# Written by the component bigscreen of x86-linux-htpc.
[Login]
HandlePowerKey=ignore
HandlePowerKeyLongPress=ignore
HandleSuspendKey=ignore
HandleHibernateKey=ignore
HandleLidSwitch=ignore
HandleLidSwitchExternalPower=ignore
HandleLidSwitchDocked=ignore
IdleAction=ignore
CONF
	install_file "${generated}" "${HTPC_LOGIND_DROPIN}"
	rm -f "${generated}"
}

# No program can suspend or hibernate the system through a masked target.
mask_sleep_targets() {
	local target
	for target in sleep suspend hibernate hybrid-sleep; do
		[[ "$(systemctl is-enabled "${target}.target" 2>/dev/null || true)" == masked ]] && continue
		systemctl mask "${target}.target" >/dev/null
		info "Masked ${target}.target"
	done
}

# Sets a key of powerdevilrc for the HTPC user. The exit code is 0 if the
# value changed.
set_powerdevil_option() {
	local profile="$1" section="$2" key="$3" value="$4"
	[[ "$(run_as_htpc_user kreadconfig6 --file powerdevilrc --group "${profile}" --group "${section}" \
		--key "${key}")" != "${value}" ]] || return 1
	run_as_htpc_user kwriteconfig6 --file powerdevilrc --group "${profile}" --group "${section}" \
		--key "${key}" "${value}"
}

# Plasma asks what to do at the power button, and the value 0 means nothing.
# The screen stays on too: it neither dims nor turns off when idle.
turn_off_plasma_power_actions() {
	local profile key changed=no
	for profile in AC Battery LowBattery; do
		for key in PowerButtonAction LidAction AutoSuspendAction; do
			if set_powerdevil_option "${profile}" SuspendAndShutdown "${key}" 0; then changed=yes; fi
		done
		for key in DimDisplayWhenIdle TurnOffDisplayWhenIdle; do
			if set_powerdevil_option "${profile}" Display "${key}" false; then changed=yes; fi
		done
	done
	# PowerDevil reads its configuration at its start.
	if [[ "${changed}" == yes ]]; then
		run_as_htpc_user systemctl --user restart plasma-powerdevil.service 2>/dev/null || true
	fi
}

ignore_sleep_shortcuts() {
	local key
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
	local output mode hdr hdr_action command folder unit
	output="$(config_require HDMI_OUTPUT)"
	mode="$(config_get DISPLAY_MODE)"
	hdr="$(config_get DISPLAY_HDR no)"
	case "${hdr}" in
	yes) hdr_action=enable ;;
	no) hdr_action=disable ;;
	*) fail "DISPLAY_HDR must be yes or no." ;;
	esac
	if [[ -n "${mode}" ]]; then
		[[ "${mode}" =~ ^[0-9]+x[0-9]+@[0-9.]+$ ]] ||
			fail "DISPLAY_MODE must be WIDTHxHEIGHT@RATE, for example 3840x2160@60."
		# The program picks the mode with the exact rate. The name of a
		# mode in kscreen-doctor has the rounded rate only.
		install_file "${component_dir}/files/display-mode" "${display_mode_program}" 0755
		command="${display_mode_program} ${output} ${mode} ${hdr}"
	else
		command="/usr/bin/kscreen-doctor output.${output}.hdr.${hdr_action}"
	fi
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
ExecStart=${command}

[Install]
WantedBy=graphical-session.target
UNIT
	chmod a+rx "${folder}"
	enable_user_service "${unit}"
	rm -rf "${folder}"
	run_as_htpc_user systemctl --user restart htpc-display.service 2>/dev/null ||
		info "The output settings apply at the next login."
}

# The KWin script files/kwin-home gives the focus back to the home screen
# and closes the launch screen after a player on its own VT. KWin loads
# the scripts with a plugin entry at each login. In a running session, the
# D-Bus call loads a new or changed script at once.
install_kwin_script() {
	local file target changed=no
	for file in metadata.json contents/code/main.js; do
		target=".local/share/kwin/scripts/${kwin_script}/${file}"
		cmp -s "${component_dir}/files/kwin-home/${file}" "$(htpc_user_home)/${target}" || changed=yes
		install_user_file "${component_dir}/files/kwin-home/${file}" "${target}"
	done
	run_as_htpc_user kwriteconfig6 --file kwinrc --group Plugins --key "${kwin_script}Enabled" true
	if [[ "${changed}" == yes ]]; then
		kwin_scripting unloadScript s "${kwin_script}" || true
	fi
	kwin_scripting start ||
		info "KWin loads the script ${kwin_script} at the next login."
}

kwin_scripting() {
	run_as_htpc_user busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting "$@" >/dev/null 2>&1
}

install_packages "${component_dir}"
[[ -f "/usr/share/wayland-sessions/${session_name}.desktop" ]] ||
	fail "The package list installed no ${session_name}.desktop."
install_autologin
configure_user_session
disable_screen_edges
ignore_power_keys_in_logind
mask_sleep_targets
turn_off_plasma_power_actions
ignore_sleep_shortcuts
install_kwin_environment
install_kwin_script
install_display_service
