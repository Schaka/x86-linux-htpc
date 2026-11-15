#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Installs UxPlay, an AirPlay receiver for audio and screen mirroring, as a
# user service of the Plasma session. docs/airplay.md describes it.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"

receiver_name() {
	local name
	name="$(config_get AIRPLAY_NAME)"
	printf '%s\n' "${name:-$(hostname -s)}"
}

# UxPlay reads one option per line. "nh" keeps the plain name, without
# "@host name" at the end. "p" sets the fixed ports that open_firewall opens.
write_receiver_options() {
	local options
	options="$(mktemp)"
	cat >"${options}" <<OPTIONS
n $(receiver_name)
nh
p
fs
vs waylandsink
OPTIONS
	install_user_file "${options}" .config/x86-linux-htpc/uxplay.conf
	rm -f "${options}"
}

# UxPlay with the option "p" listens on these ports. The desktop zones of
# firewalld allow the high ports, but other zones block them. AirPlay finds
# the receiver with mDNS.
open_firewall() {
	local zone port
	command -v firewall-cmd >/dev/null 2>&1 && firewall-cmd --state >/dev/null 2>&1 || return 0
	zone="$(firewall-cmd --get-default-zone)"
	for port in 7000/tcp 7001/tcp 7100/tcp 6000/udp 6001/udp 7011/udp; do
		firewall_allow "${zone}" "--add-port=${port}" "--query-port=${port}" "port ${port}"
	done
	firewall_allow "${zone}" --add-service=mdns --query-service=mdns mDNS
}

install_packages "${component_dir}"
write_receiver_options
enable_user_service "${component_dir}/files/htpc-airplay.service"
open_firewall
# The service reads its options at the start. The restart works only while
# the user is logged in. Otherwise the service starts with the next login.
run_as_htpc_user systemctl --user restart htpc-airplay.service 2>/dev/null || true
