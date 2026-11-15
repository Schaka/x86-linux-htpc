#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

program=/usr/local/libexec/x86-linux-htpc/htpc-control
port="$(config_get CONTROL_API_PORT 8780)"
home="$(htpc_user_home)"

program_runs() {
	"${program}" --version >/dev/null
}

service_enabled() {
	[[ -L "${home}/.config/systemd/user/graphical-session.target.wants/htpc-control.service" ]]
}

service_active() {
	run_as_htpc_user systemctl --user is-active --quiet htpc-control.service
}

api_answers() {
	curl -fsS -m 5 "http://127.0.0.1:${port}/health" | grep -q '"status":"ok"'
}

renderer_answers() {
	curl -fsS -m 5 "http://127.0.0.1:${port}/dlna/device.xml" | grep -q 'MediaRenderer'
}

dial_answers() {
	curl -fsS -m 5 -i "http://127.0.0.1:${port}/dial/device.xml" | grep -qi '^Application-URL:'
}

check "The program htpc-control runs" program_runs
check "The user service starts with the Plasma session" service_enabled
check "The user service runs" service_active
check "The API answers on port ${port}" api_answers
if [[ "$(config_get CONTROL_DLNA yes)" == yes ]]; then
	check "The DLNA renderer answers" renderer_answers
fi

# The service starts DIAL only when VacuumTube has its launcher entry.
if [[ "$(config_get CONTROL_DIAL yes)" == yes && -f "${home}/.local/share/applications/htpc-youtube-tv.desktop" ]]; then
	check "The DIAL launcher for VacuumTube answers" dial_answers
fi

checks_done
