#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Logs jellyfin-mpv-shim in to the Jellyfin server with Quick Connect.
#
#   sudo components/jellyfin-mpv-shim/login.sh
#
# The script prints a code. Approve it within 5 minutes in another Jellyfin
# client: user menu, Quick Connect. No password goes into a file. The server
# address comes from JELLYFIN_SERVER_URL in the configuration file.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

server="$(config_require JELLYFIN_SERVER_URL)"
shim="$(htpc_user_home)/.local/bin/jellyfin-mpv-shim"
[[ -x "${shim}" ]] || fail "jellyfin-mpv-shim is not installed. Run ./install.sh jellyfin-mpv-shim first."

logged_in() {
	python3 "$(dirname "${BASH_SOURCE[0]}")/has-login.py" "${users_file}"
}

# "jellyfin-mpv-shim stop" ends a copy that runs as a cast target. A copy
# that still waits for a login ignores it and SIGTERM, so SIGKILL ends it.
stop_all_copies() {
	run_as_htpc_user systemctl --user stop jellyfin-mpv-shim.service
	run_as_htpc_user timeout 15 "${shim}" stop >/dev/null 2>&1 || true
	if pgrep -u "$(htpc_user)" -f "${shim}" >/dev/null; then
		pkill -KILL -u "$(htpc_user)" -f "${shim}" || true
	fi
}

users_file="$(htpc_user_home)/.config/jellyfin-mpv-shim/users.json"

# Only one copy of the shim can run for a user, so all copies must stop for
# the login, and the service starts again afterwards.
stop_all_copies

# After a successful login the shim keeps running as a cast target. The
# watcher ends it as soon as users.json has a login, so the service can
# take over. stdin is closed, so a failed login ends instead of waiting.
run_as_htpc_user "${shim}" --server "${server}" --quick-connect </dev/null &
login_pid=$!
while kill -0 "${login_pid}" 2>/dev/null; do
	logged_in && break
	sleep 2
done
stop_all_copies
wait "${login_pid}" 2>/dev/null || true

run_as_htpc_user systemctl --user start jellyfin-mpv-shim.service
if logged_in; then
	info "The login worked. The shim runs as a service again."
else
	fail "The login did not finish. Run the script again for a new code."
fi
