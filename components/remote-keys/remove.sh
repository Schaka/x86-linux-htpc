#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

systemctl disable --now x86-linux-htpc-air-mouse.service 2>/dev/null || true
remove_file /etc/systemd/system/x86-linux-htpc-air-mouse.service
remove_file /usr/local/libexec/x86-linux-htpc/air-mouse
systemctl daemon-reload
if [[ -e /etc/udev/hwdb.d/70-x86-linux-htpc-remote.hwdb ]]; then
	remove_file /etc/udev/hwdb.d/70-x86-linux-htpc-remote.hwdb
	systemd-hwdb update
	udevadm trigger --subsystem-match=input --action=change
fi
