#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Makes the power key of a remote turn the TV and the AV receiver on and
# off. The service reads the keys in POWER_KEYS, and the program htpc-power
# does the work. docs/power-key.md explains the two routes: Home Assistant
# through a webhook, or the HTPC itself with Wake-on-LAN and a command.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
service=x86-linux-htpc-power-key.service

check_configuration() {
	local key webhook
	webhook="$(config_get POWER_WEBHOOK_URL)"
	for key in $(config_get POWER_KEYS KEY_SLEEP); do
		[[ "${key}" =~ ^KEY_[A-Z0-9_]+$ ]] ||
			fail "POWER_KEYS has the value ${key}. Use the names of evdev keys, for example KEY_SLEEP."
	done
	# The HTPC must stay on. The keys of a remote are safe only if the
	# component bigscreen makes the system ignore the power and sleep keys.
	if ! logind_ignores HandlePowerKey || ! logind_ignores HandleSuspendKey; then
		fail "The system does not ignore the power and sleep keys yet. Install the component bigscreen first."
	fi
	[[ "$(config_get POWER_STOP_MEDIA yes)" =~ ^(yes|no)$ ]] ||
		fail "POWER_STOP_MEDIA must be yes or no."
	[[ -z "${webhook}" || "${webhook}" =~ ^https?://[^[:space:]]+$ ]] ||
		fail "POWER_WEBHOOK_URL must start with http:// or https://."
	if [[ -z "${webhook}" ]]; then
		[[ -n "$(config_get POWER_OFF_COMMAND)" ]] ||
			warn "POWER_WEBHOOK_URL and POWER_OFF_COMMAND are empty, so the key cannot turn anything off."
		[[ -n "$(config_get WAKE_ON_LAN_MACS)" || -n "$(config_get POWER_ON_COMMAND)" ]] ||
			warn "WAKE_ON_LAN_MACS and POWER_ON_COMMAND are empty, so the key cannot turn anything on."
	fi
}

install_packages "${component_dir}"
check_configuration
install_file "${component_dir}/files/htpc-power" /usr/local/bin/htpc-power 0755
install_file "${component_dir}/files/${service}" "/etc/systemd/system/${service}"
systemctl daemon-reload
if [[ -n "$(config_get POWER_KEYS KEY_SLEEP)" ]]; then
	systemctl enable "${service}"
	systemctl restart "${service}"
else
	systemctl disable --now "${service}" 2>/dev/null || true
	info "POWER_KEYS is empty, so the power key service does not run."
fi
