#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

run_as_htpc_user systemctl --user stop htpc-airplay.service 2>/dev/null || true
disable_user_service htpc-airplay.service
remove_file "$(htpc_user_home)/.config/x86-linux-htpc/uxplay.conf"
if command -v firewall-cmd >/dev/null 2>&1 && firewall-cmd --state >/dev/null 2>&1; then
	zone="$(firewall-cmd --get-default-zone)"
	for port in 7000/tcp 7001/tcp 7100/tcp 6000/udp 6001/udp 7011/udp; do
		firewall_deny "${zone}" "--remove-port=${port}" "--query-port=${port}" "port ${port}"
	done
fi
