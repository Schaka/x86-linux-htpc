#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

systemctl disable --now x86-linux-htpc-power-key.service 2>/dev/null || true
remove_file /etc/systemd/system/x86-linux-htpc-power-key.service
remove_file /usr/local/bin/htpc-power
systemctl daemon-reload
