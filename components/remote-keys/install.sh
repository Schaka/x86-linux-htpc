#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Gives the air mouse remotes in AIR_MOUSE_USB_IDS a TV-friendly OK and
# Back. An air mouse sends OK as the left and Back as the right mouse
# button. The service files/air-mouse makes OK a click while the pointer is
# in use, and Enter otherwise. Back is always the key Back. docs/tv-control.md
# explains it.
#
# AIR_MOUSE_USB_IDS in the configuration file lists the USB IDs of the
# receivers, for example 1234:abcd. "lsusb" shows them.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
service=x86-linux-htpc-air-mouse.service
# The service needs the raw mouse buttons. A hwdb key map for the receiver
# in this file turns them into keys, so the install removes the file.
old_hwdb_file=/etc/udev/hwdb.d/70-x86-linux-htpc-remote.hwdb

check_ids() {
	local id
	for id in $(config_get AIR_MOUSE_USB_IDS); do
		[[ "${id}" =~ ^[0-9a-fA-F]{4}:[0-9a-fA-F]{4}$ ]] ||
			fail "AIR_MOUSE_USB_IDS has the value ${id}. Use VENDOR:PRODUCT, for example 1234:abcd."
	done
}

# The kernel applies a removed hwdb map only to devices that appear again,
# so the receiver connects again.
reconnect_receivers() {
	local id usb
	for id in $(config_get AIR_MOUSE_USB_IDS); do
		for usb in /sys/bus/usb/devices/*; do
			[[ -f "${usb}/idVendor" && "$(cat "${usb}/idVendor")" == "${id%%:*}" &&
				"$(cat "${usb}/idProduct")" == "${id##*:}" ]] || continue
			echo 0 >"${usb}/authorized"
			sleep 1
			echo 1 >"${usb}/authorized"
		done
	done
	udevadm settle
}

remove_old_key_map() {
	[[ -e "${old_hwdb_file}" ]] || return 0
	remove_file "${old_hwdb_file}"
	systemd-hwdb update
	reconnect_receivers
}

install_packages "${component_dir}"
check_ids
remove_old_key_map
install_file "${component_dir}/files/air-mouse" /usr/local/libexec/x86-linux-htpc/air-mouse 0755
install_file "${component_dir}/files/${service}" "/etc/systemd/system/${service}"
systemctl daemon-reload
if [[ -n "$(config_get AIR_MOUSE_USB_IDS)" ]]; then
	systemctl enable "${service}"
	systemctl restart "${service}"
else
	systemctl disable --now "${service}" 2>/dev/null || true
	info "AIR_MOUSE_USB_IDS is empty, so the air mouse service does not run."
fi
