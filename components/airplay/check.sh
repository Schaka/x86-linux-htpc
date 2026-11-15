#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

home="$(htpc_user_home)"

program_installed() {
	package_installed uxplay
}

options_installed() {
	[[ -f "${home}/.config/x86-linux-htpc/uxplay.conf" ]]
}

service_enabled() {
	[[ -L "${home}/.config/systemd/user/graphical-session.target.wants/htpc-airplay.service" ]]
}

service_active() {
	run_as_htpc_user systemctl --user is-active --quiet htpc-airplay.service
}

# The receiver must answer mDNS queries, or no phone finds it. A new start
# needs a moment to announce itself, so the check tries a few times.
service_announced() {
	local name announced
	name="$(config_get AIRPLAY_NAME)"
	name="${name:-$(hostname -s)}"
	for _ in 1 2 3 4 5; do
		# grep -q would end early, and avahi-browse would fail with pipefail.
		announced="$(avahi-browse -t _airplay._tcp)"
		[[ "${announced}" == *" ${name} "* ]] && return 0
		sleep 1
	done
	return 1
}

check "The package uxplay is installed" program_installed
check "The options file of the receiver exists" options_installed
check "The user service starts with the Plasma session" service_enabled
check "The user service runs" service_active
check "The receiver announces itself with mDNS" service_announced

checks_done
