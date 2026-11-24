#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

prefix=/opt/x86-linux-htpc/cb1

mpv_runs() {
	"${prefix}/bin/mpv" --version 2>/dev/null | grep -q "^mpv"
}

libraries_found() {
	! ldd "${prefix}/bin/mpv" 2>/dev/null | grep -q "not found"
}

uses_own_libraries() {
	ldd "${prefix}/bin/mpv" 2>/dev/null | grep -q "libplacebo.* => ${prefix}/lib/"
}

guard_enabled() {
	[[ -L "$(htpc_user_home)/.config/systemd/user/graphical-session.target.wants/htpc-screen-guard.service" ]]
}

guard_active() {
	run_as_htpc_user systemctl --user is-active --quiet htpc-screen-guard.service
}

check "The CB1 mpv starts" mpv_runs
check "The CB1 mpv finds all its libraries" libraries_found
check "The CB1 mpv uses the patched libplacebo" uses_own_libraries
check "The screen guard starts with the Plasma session" guard_enabled
check "The screen guard runs" guard_active

checks_done
