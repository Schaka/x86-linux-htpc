#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Removes the Favorites file and shows the other rows of the home screen
# again.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

home="$(htpc_user_home)"
if run_as_htpc_user systemctl --user is-active --quiet plasma-plasmashell.service 2>/dev/null; then
	run_as_htpc_user systemctl --user stop plasma-plasmashell.service
	remove_file "${home}/.config/bigscreen-favs"
	run_as_htpc_user systemctl --user start plasma-plasmashell.service
	sleep 5
	for row in Recent Applications Games; do
		run_as_htpc_user busctl --user call org.kde.biglauncher /BigLauncher org.kde.biglauncher "setShow${row}" b true || true
	done
else
	remove_file "${home}/.config/bigscreen-favs"
fi
