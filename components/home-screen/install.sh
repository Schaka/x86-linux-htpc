#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Sets the apps of the Plasma Bigscreen home screen from HOME_SCREEN_APPS.
# The Favorites row shows these apps, and the rows Recent, Applications and
# Games are off. Without HOME_SCREEN_APPS, the home screen stays as it is.
#
# The component comes last in the install order, because the other
# components add the desktop entries of the apps.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
rows=(Recent Applications Games)

launcher_call() {
	run_as_htpc_user busctl --user call org.kde.biglauncher /BigLauncher org.kde.biglauncher "$@"
}

# The home screen applies and saves the rows through its D-Bus interface.
# A row switch in its configuration file does not reach a running home
# screen.
hide_other_rows() {
	local row
	for _ in $(seq 20); do
		launcher_call showRecentActive >/dev/null 2>&1 && break
		sleep 1
	done
	for row in "${rows[@]}"; do
		launcher_call "setShow${row}" b false ||
			warn "The home screen does not run, so the row ${row} stays on."
	done
}

# The Home key of a remote shows the home screen over an open app, for
# example over Android. Inside the CB1 mpv, the same key goes back to
# Plasma. 16777360 is Qt::Key_HomePage.
bind_home_key() {
	run_as_htpc_user busctl --user call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel \
		setForeignShortcutKeys "asa(ai)" 4 plasmashell "Toggle Bigscreen Home Screen" "" "" 1 1 16777360 ||
		warn "KWin does not run, so the Home key stays unbound."
}

apps="$(config_get HOME_SCREEN_APPS)"
if [[ -z "${apps}" ]]; then
	info "HOME_SCREEN_APPS is empty. The home screen stays as it is."
	exit 0
fi

set_favorites() {
	# shellcheck disable=SC2086 # The value is a list of desktop IDs.
	run_as_htpc_user python3 "${component_dir}/files/home-apps.py" "$(htpc_user_home)/.config/bigscreen-favs" ${apps}
}

# On a fresh machine the user has not logged in yet, so no session exists.
# The favorites are only a file then. The rows and the Home key need the
# running session, so the install must run again after the first login.
if ! run_as_htpc_user systemctl --user is-active --quiet plasma-plasmashell.service 2>/dev/null; then
	set_favorites
	warn "The Plasma session of $(htpc_user) does not run. The favorites are set. Log in at the TV, and run sudo ./install.sh home-screen again to hide the rows and to bind the Home key."
	exit 0
fi

# The home screen saves the rows into the file that the check reads only
# when the shell ends. So the rows change first, and the shell stops after.
hide_other_rows
# The home screen reads its favorites only when it starts, and it writes
# them back when it ends. So the Plasma shell stops before the change.
run_as_htpc_user systemctl --user stop plasma-plasmashell.service
set_favorites
run_as_htpc_user systemctl --user start plasma-plasmashell.service
bind_home_key
