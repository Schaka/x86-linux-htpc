#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

service=x86-linux-htpc-power-key.service
program=/usr/local/bin/htpc-power

program_installed() {
	[[ -x "${program}" ]]
}

service_runs() {
	systemctl is-enabled --quiet "${service}" && systemctl is-active --quiet "${service}"
}

# A webhook leaves the route to Home Assistant. Without it, the key needs a
# command for the off, and Wake-on-LAN or a command for the on.
can_turn_off() {
	[[ -n "$(config_get POWER_WEBHOOK_URL)" || -n "$(config_get POWER_OFF_COMMAND)" ]]
}

can_turn_on() {
	[[ -n "$(config_get POWER_WEBHOOK_URL)" || -n "$(config_get WAKE_ON_LAN_MACS)" ||
		-n "$(config_get POWER_ON_COMMAND)" ]]
}

# With a webhook, Home Assistant knows the state. Otherwise the program must
# read it: "state" exits with 0 or 1, and with 2 when it does not know.
state_known() {
	[[ -n "$(config_get POWER_WEBHOOK_URL)" ]] && return 0
	local status=0
	run_as_htpc_user "${program}" state || status=$?
	[[ "${status}" -le 1 ]]
}

# The HTPC must stay on. The power and sleep keys of a remote must do
# nothing but the job of this component.
system_ignores_power_keys() {
	logind_ignores HandlePowerKey && logind_ignores HandleSuspendKey
}

check "htpc-power is installed" program_installed
check "The system ignores the power and sleep keys, so they cannot stop the HTPC" system_ignores_power_keys
if [[ -z "$(config_get POWER_KEYS KEY_SLEEP)" ]]; then
	check_pass "POWER_KEYS is empty, so no service reads the keys"
else
	check "The power key service runs" service_runs
fi
check "The power key can turn the devices off" can_turn_off
check "The power key can turn the devices on" can_turn_on
check "The power state is known" state_known

checks_done
