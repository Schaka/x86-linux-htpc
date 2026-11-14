#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

run_as_htpc_user systemctl --user stop htpc-control.service 2>/dev/null || true
disable_user_service htpc-control.service
remove_file /usr/local/libexec/x86-linux-htpc/htpc-control
if command -v firewall-cmd >/dev/null 2>&1 && firewall-cmd --state >/dev/null 2>&1; then
	port="$(config_get CONTROL_API_PORT 8780)"
	zone="$(firewall-cmd --get-default-zone)"
	# No reload: it would drop the runtime rules of other programs.
	for mode in "" --permanent; do
		# shellcheck disable=SC2086 # mode is empty or one option
		if firewall-cmd ${mode} --zone="${zone}" --query-port="${port}/tcp" >/dev/null; then
			firewall-cmd ${mode} --zone="${zone}" --remove-port="${port}/tcp" >/dev/null
		fi
	done
	info "Closed TCP port ${port} in the firewall zone ${zone}"
fi
