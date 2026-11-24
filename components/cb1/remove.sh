#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

run_as_htpc_user systemctl --user stop htpc-screen-guard.service 2>/dev/null || true
disable_user_service htpc-screen-guard.service
remove_file /usr/local/libexec/x86-linux-htpc/screen-guard
rm -rf /opt/x86-linux-htpc/cb1
info "Removed /opt/x86-linux-htpc/cb1"
