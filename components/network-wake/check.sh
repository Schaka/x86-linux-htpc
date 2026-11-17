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

# The setting is in the connection. For a connection that is up, the card
# must have it too. ethtool shows the wake option of a card to root only.
htpc_accepts_wake() {
	local uuid type device
	[[ "$(config_get HTPC_WAKE_ON_LAN yes)" == yes ]] || return 0
	while IFS=: read -r uuid type; do
		[[ "${type}" == 802-3-ethernet ]] || continue
		[[ "$(nmcli -g 802-3-ethernet.wake-on-lan connection show uuid "${uuid}")" == magic ]] || return 1
		device="$(nmcli -g GENERAL.DEVICES connection show uuid "${uuid}")"
		if [[ -n "${device}" && "${EUID}" -eq 0 ]]; then
			ethtool "${device}" | grep -q "Wake-on: .*g" || return 1
		fi
	done < <(nmcli -t -f UUID,TYPE connection show)
}

check "WAKE_ON_LAN_MACS lists MAC addresses" macs_valid
check "The network cards of the HTPC accept Wake-on-LAN" htpc_accepts_wake
check "The Wake-on-LAN sender is installed" sender_installed
check "The user service wakes the TV at login" service_enabled
check "The mpv script wakes the TV at the start of each video" mpv_script_installed
check "The sender runs as the HTPC user" sender_runs

checks_done
