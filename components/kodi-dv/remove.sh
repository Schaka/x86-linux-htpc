#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Removes Kodi, the launcher and the tile. The data of Kodi in ~/.kodi
# stays.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

remove_file "$(htpc_user_home)/.local/share/applications/htpc-kodi.desktop"
remove_file /usr/local/libexec/x86-linux-htpc/kodi-dv
rm -rf /opt/x86-linux-htpc/kodi
info "Removed /opt/x86-linux-htpc/kodi"
