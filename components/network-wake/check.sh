#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

home="$(htpc_user_home)"

macs_valid() {
	local mac macs
	macs="$(config_get WAKE_ON_LAN_MACS)"
	[[ -n "${macs}" ]] || return 1
	for mac in ${macs}; do
		[[ "${mac}" =~ ^([0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}$ ]] || return 1
	done
}

sender_installed() {
	[[ -x /usr/local/libexec/x86-linux-htpc/wake-devices ]]
}

service_enabled() {
	[[ -L "${home}/.config/systemd/user/graphical-session.target.wants/htpc-wake.service" ]]
}

mpv_script_installed() {
	[[ -f "${home}/.config/jellyfin-mpv-shim/scripts/htpc-wake.lua" ]]
}

# The sender reads the configuration as the HTPC user, so the check runs it
# as that user. The TV ignores the packets while it is on.
sender_runs() {
	run_as_htpc_user /usr/local/libexec/x86-linux-htpc/wake-devices
}

check "WAKE_ON_LAN_MACS lists MAC addresses" macs_valid
check "The Wake-on-LAN sender is installed" sender_installed
check "The user service wakes the TV at login" service_enabled
check "The mpv script wakes the TV at the start of each video" mpv_script_installed
check "The sender runs as the HTPC user" sender_runs

checks_done
