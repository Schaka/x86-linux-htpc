#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

session_name="plasma-bigscreen-wayland"
home="$(htpc_user_home)"

session_file_exists() {
	[[ -f "/usr/share/wayland-sessions/${session_name}.desktop" ]]
}

autologin_configured() {
	grep -qx "User=$(htpc_user)" /etc/plasmalogin.conf.d/x86-linux-htpc.conf /etc/sddm.conf.d/x86-linux-htpc.conf 2>/dev/null &&
		grep -qx "Session=${session_name}" /etc/plasmalogin.conf.d/x86-linux-htpc.conf /etc/sddm.conf.d/x86-linux-htpc.conf 2>/dev/null
}

user_config_is() {
	local file="$1" key="$2" value="$3"
	grep -qx "${key}=${value}" "${home}/.config/${file}" 2>/dev/null
}

bigscreen_session_runs() {
	# The process name has more than the 15 characters that pgrep -x
	# compares, so the check reads the command line.
	pgrep -u "$(htpc_user)" -f '^(/usr/bin/sh )?/usr/bin/plasma-bigscreen-wayland' >/dev/null
}

check "The Bigscreen session file exists" session_file_exists
check "The display manager logs in $(htpc_user) to Bigscreen" autologin_configured
check "The screen lock is off" user_config_is kscreenlockerrc Autolock false
check "The automatic suspend is off" user_config_is powerdevilrc AutoSuspendAction 0
check "KWallet is off" user_config_is kwalletrc Enabled false
if [[ "$(config_get SCREEN_EDGES off)" == off ]]; then
	check "The overview has no screen corner" user_config_is kwinrc BorderActivate 9
fi

# Plasma writes its shortcut file some time after a change, so the check
# asks the running shortcut service first.
sleep_keys_ignored() {
	logind_ignores HandleSuspendKey || return 1
	local keys
	if keys="$(run_as_htpc_user busctl --user call org.kde.kglobalaccel /kglobalaccel \
		org.kde.KGlobalAccel shortcutKeys as 4 org_kde_powerdevil Sleep "" "" 2>/dev/null)"; then
		[[ "${keys}" == "a(ai) 0" ]]
	else
		grep -q "^Sleep=none," "${home}/.config/kglobalshortcutsrc"
	fi
}

check "The sleep key of a remote does not suspend the HTPC" sleep_keys_ignored

logind_ignores_keys() {
	local key
	for key in HandlePowerKey HandleSuspendKey HandleHibernateKey HandleLidSwitch IdleAction; do
		logind_ignores "${key}" || return 1
	done
}

sleep_targets_masked() {
	local target
	for target in sleep suspend hibernate hybrid-sleep; do
		[[ "$(systemctl is-enabled "${target}.target" 2>/dev/null)" == masked ]] || return 1
	done
}

plasma_power_actions_off() {
	local profile key
	for profile in AC Battery LowBattery; do
		for key in PowerButtonAction LidAction AutoSuspendAction; do
			[[ "$(kreadconfig6 --file "${home}/.config/powerdevilrc" --group "${profile}" \
				--group SuspendAndShutdown --key "${key}")" == 0 ]] || return 1
		done
		for key in DimDisplayWhenIdle TurnOffDisplayWhenIdle; do
			[[ "$(kreadconfig6 --file "${home}/.config/powerdevilrc" --group "${profile}" \
				--group Display --key "${key}")" == false ]] || return 1
		done
	done
}

check "logind ignores the power, sleep and lid keys and the idle time" logind_ignores_keys
check "The sleep targets are masked, so nothing suspends the HTPC" sleep_targets_masked
check "Plasma does nothing at the power button, the lid or the idle time, and keeps the screen on" plasma_power_actions_off

kwin_composes_fullscreen() {
	grep -qx "KWIN_DRM_NO_DIRECT_SCANOUT=1" "${home}/.config/environment.d/x86-linux-htpc-kwin.conf"
}

check "KWin composes full-screen apps itself" kwin_composes_fullscreen
check "The Bigscreen session of $(htpc_user) runs" bigscreen_session_runs

display_service_enabled() {
	[[ -L "${home}/.config/systemd/user/graphical-session.target.wants/htpc-display.service" ]]
}

# Compares the output of the running session with DISPLAY_MODE and
# DISPLAY_HDR. The CB1 mpv on its own VT does not change the mode of KWin.
display_matches_config() {
	local output mode hdr
	output="$(config_get HDMI_OUTPUT)"
	mode="$(config_get DISPLAY_MODE)"
	hdr="$(config_get DISPLAY_HDR no)"
	run_as_htpc_user env WAYLAND_DISPLAY="$(wayland_socket)" kscreen-doctor -j 2>/dev/null |
		python3 -c '
import json, re, sys
output, mode, hdr = sys.argv[1:]
for o in json.load(sys.stdin)["outputs"]:
    if o["name"] != output:
        continue
    if o["hdr"] != (hdr == "yes"):
        sys.exit(1)
    if mode:
        current = next(m for m in o["modes"] if m["id"] == o["currentModeId"])
        width, height, rate = re.fullmatch(r"(\d+)x(\d+)@([\d.]+)", mode).groups()
        if (current["size"]["width"], current["size"]["height"]) != (int(width), int(height)):
            sys.exit(1)
        if abs(current["refreshRate"] - float(rate)) > 0.5:
            sys.exit(1)
    sys.exit(0)
sys.exit(1)
' "${output}" "${mode}" "${hdr}"
}

wayland_socket() {
	local uid
	uid="$(id -u "$(htpc_user)")"
	find "/run/user/${uid}" -maxdepth 1 -type s -name 'wayland-[0-9]*' -printf '%f\n' | sort | head -n 1
}

kwin_script=x86-linux-htpc-home

kwin_script_enabled() {
	[[ -f "${home}/.local/share/kwin/scripts/${kwin_script}/contents/code/main.js" ]] &&
		user_config_is kwinrc "${kwin_script}Enabled" true
}

kwin_script_loaded() {
	[[ "$(run_as_htpc_user busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting \
		isScriptLoaded s "${kwin_script}" 2>/dev/null)" == "b true" ]]
}

check "The KWin script keeps the focus on the home screen" kwin_script_enabled
if bigscreen_session_runs; then
	check "KWin runs the home screen script" kwin_script_loaded
fi
check "The output settings apply at each login" display_service_enabled
if bigscreen_session_runs; then
	check "The output has DISPLAY_MODE and DISPLAY_HDR" display_matches_config
fi

checks_done
