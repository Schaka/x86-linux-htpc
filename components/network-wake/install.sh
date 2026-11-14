#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Wakes the TV over the network (Wake-on-LAN) at login and at the start of
# each video in jellyfin-mpv-shim and VacuumTube. Then a cast turns on a TV
# in standby.
#
# WAKE_ON_LAN_MACS in the configuration file lists the MAC addresses. A TV
# with a cable and Wi-Fi has two addresses. List both, because the TV
# listens on the one that it used last.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
sender=/usr/local/libexec/x86-linux-htpc/wake-devices
shim_scripts=.config/jellyfin-mpv-shim/scripts

check_macs() {
	local mac
	for mac in $(config_get WAKE_ON_LAN_MACS); do
		[[ "${mac}" =~ ^([0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}$ ]] ||
			fail "WAKE_ON_LAN_MACS has an entry that is not a MAC address: ${mac}"
	done
	[[ -n "$(config_get WAKE_ON_LAN_MACS)" ]] ||
		warn "WAKE_ON_LAN_MACS is empty, so the component sends no packets."
}

install_packages "${component_dir}"
check_macs
install_file "${component_dir}/files/wake-devices" "${sender}" 0755
enable_user_service "${component_dir}/files/htpc-wake.service"
# The CB1 mpv and the libmpv of the shim both load the scripts of this
# folder.
install_user_file "${component_dir}/files/htpc-wake.lua" "${shim_scripts}/htpc-wake.lua"
