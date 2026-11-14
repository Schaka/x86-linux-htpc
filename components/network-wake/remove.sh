#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

disable_user_service htpc-wake.service
remove_file "$(htpc_user_home)/.config/jellyfin-mpv-shim/scripts/htpc-wake.lua"
remove_file /usr/local/libexec/x86-linux-htpc/wake-devices
