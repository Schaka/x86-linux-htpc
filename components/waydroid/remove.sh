#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Stops the Waydroid container. The package, the Android image and the data
# of the apps stay, because a new download takes a long time. Remove them
# with dnf and by deleting /var/lib/waydroid.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

run_as_htpc_user systemctl --user stop waydroid-session.service 2>/dev/null || true
disable_user_service waydroid-session.service
systemctl disable --now waydroid-container.service 2>/dev/null || true
if command -v semodule >/dev/null && semodule -l | grep -qx htpc-waydroid; then
	semodule -r htpc-waydroid
	rm -f /var/lib/x86-linux-htpc/htpc-waydroid.pp
	info "Removed the SELinux module htpc-waydroid"
fi
