#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

home="$(htpc_user_home)"
shim_conf="${home}/.config/jellyfin-mpv-shim"

shim_installed() {
	[[ -x "${home}/.local/bin/jellyfin-mpv-shim" ]]
}

mpris_available() {
	"${home}/.local/share/pipx/venvs/jellyfin-mpv-shim/bin/python" -c 'import gi' 2>/dev/null
}

libmpv_present() {
	local libraries
	libraries="$(ldconfig -p)"
	grep -q 'libmpv\.so\.2' <<<"${libraries}"
}

mpv_conf_includes_options() {
	grep -qxF 'include="~~/x86-linux-htpc.conf"' "${shim_conf}/mpv.conf" &&
		[[ -f "${shim_conf}/x86-linux-htpc.conf" ]]
}

service_enabled() {
	[[ -L "${home}/.config/systemd/user/graphical-session.target.wants/jellyfin-mpv-shim.service" ]]
}

service_running() {
	run_as_htpc_user systemctl --user is-active --quiet jellyfin-mpv-shim.service
}

logged_in() {
	python3 "$(dirname "${BASH_SOURCE[0]}")/has-login.py" "${shim_conf}/users.json"
}

cb1_mpv_present() {
	[[ -x /opt/x86-linux-htpc/cb1/bin/mpv ]]
}

cb1_scripts_present() {
	[[ -f "${shim_conf}/scripts/htpc-output.lua" && -x "${shim_conf}/scripts/htpc-screen.run" ]]
}

# The running user manager keeps the groups of its start, so a new group
# applies only after the next login.
cb1_groups_active() {
	local pid groups
	pid="$(pgrep -u "$(htpc_user)" -x systemd | head -n 1)"
	[[ -n "${pid}" ]] || return 1
	groups="$(sed -n 's/^Groups:\s*//p' "/proc/${pid}/status")"
	for group in video input audio; do
		grep -qw "$(getent group "${group}" | cut -d: -f3)" <<<"${groups}" || return 1
	done
}

refresh_script_present() {
	[[ -f "${shim_conf}/scripts/htpc-refresh.lua" ]]
}

shim_uses_cb1_mpv() {
	python3 -c 'import json, sys
c = json.load(open(sys.argv[1]))
sys.exit(0 if c.get("mpv_ext") and c.get("mpv_ext_path") == sys.argv[2] else 1)' \
		"${shim_conf}/conf.json" /opt/x86-linux-htpc/cb1/bin/mpv
}

check "jellyfin-mpv-shim is installed for the HTPC user" shim_installed
check "libmpv is installed" libmpv_present
check "The shim can load PyGObject for its MPRIS media control" mpris_available
check "mpv.conf includes the options of x86-linux-htpc" mpv_conf_includes_options
check "The user service starts with the Plasma session" service_enabled

if [[ "$(config_get JELLYFIN_MPV system)" == cb1 ]]; then
	check "The CB1 mpv is installed" cb1_mpv_present
	check "The shim starts the CB1 mpv" shim_uses_cb1_mpv
	check "The scripts of the CB1 mpv are installed" cb1_scripts_present
	check "The session of the HTPC user has the groups video, input and audio" cb1_groups_active
elif [[ "$(config_get DISPLAY_MATCH_RATE yes)" == yes ]]; then
	check "The frame rate script of mpv is installed" refresh_script_present
fi

if logged_in; then
	check_pass "jellyfin-mpv-shim has a server login"
else
	check_fail "jellyfin-mpv-shim has no server login. Run: sudo components/jellyfin-mpv-shim/login.sh"
fi

if run_as_htpc_user systemctl --user is-active --quiet graphical-session.target 2>/dev/null; then
	check "The user service runs" service_running
else
	warn "The Plasma session of the HTPC user does not run, so the check of the running service is skipped."
fi

checks_done
