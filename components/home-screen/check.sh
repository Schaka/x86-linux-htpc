#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

home="$(htpc_user_home)"
apps="$(config_get HOME_SCREEN_APPS)"

if [[ -z "${apps}" ]]; then
	check_pass "HOME_SCREEN_APPS is empty, so the home screen keeps its own rows"
	checks_done
fi

favorite_present() {
	grep -qx "storageId=$1" "${home}/.config/bigscreen-favs" 2>/dev/null
}

rows_hidden() {
	local file="${home}/.config/plasma-org.kde.plasma.bigscreen-appletsrc"
	grep -qx "showApplications=false" "${file}" && grep -qx "showGames=false" "${file}" &&
		grep -qx "showRecent=false" "${file}"
}

for app in ${apps}; do
	check "The home screen shows ${app}" favorite_present "${app}"
done
home_key_bound() {
	grep -q '^Toggle Bigscreen Home Screen=Home Page,' "${home}/.config/kglobalshortcutsrc"
}

check "The home screen hides the rows Recent, Applications and Games" rows_hidden
check "The Home key shows the home screen" home_key_bound

checks_done
