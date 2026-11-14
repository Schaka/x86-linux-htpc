#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

remove_file "$(htpc_user_home)/.config/wireplumber/wireplumber.conf.d/x86-linux-htpc-hdmi.conf"
run_as_htpc_user systemctl --user restart wireplumber.service 2>/dev/null || true
