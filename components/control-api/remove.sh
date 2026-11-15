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
	firewall_deny "${zone}" "--remove-port=${port}/tcp" "--query-port=${port}/tcp" "TCP port ${port}"
fi
