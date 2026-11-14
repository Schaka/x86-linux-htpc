#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

service=x86-linux-htpc-air-mouse.service

if [[ -z "$(config_get AIR_MOUSE_USB_IDS)" ]]; then
	check_pass "AIR_MOUSE_USB_IDS is empty, so no remote needs the air mouse service"
	checks_done
fi

no_old_key_map() {
	[[ ! -e /etc/udev/hwdb.d/70-x86-linux-htpc-remote.hwdb ]]
}

service_runs() {
	systemctl is-enabled --quiet "${service}" && systemctl is-active --quiet "${service}"
}

# The service makes the device a moment after its start.
virtual_device_present() {
	local _
	for _ in 1 2 3 4 5; do
		grep -q 'N: Name="x86-linux-htpc air mouse"' /proc/bus/input/devices && return 0
		sleep 1
	done
	return 1
}

check "No fixed key map changes the mouse buttons" no_old_key_map
check "The air mouse service runs" service_runs
check "The virtual air mouse device exists" virtual_device_present

checks_done
