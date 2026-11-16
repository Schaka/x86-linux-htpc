#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Removes the automatic login and the KWin script of the home screen. The
# packages and the options of the user stay, because the user can also use
# them without this project.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

remove_file /etc/plasmalogin.conf.d/x86-linux-htpc.conf
remove_file /etc/sddm.conf.d/x86-linux-htpc.conf
remove_file /etc/systemd/logind.conf.d/x86-linux-htpc.conf
remove_file "$(htpc_user_home)/.config/environment.d/x86-linux-htpc-kwin.conf"
disable_user_service htpc-display.service

kwin_script=x86-linux-htpc-home
run_as_htpc_user busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting \
	unloadScript s "${kwin_script}" >/dev/null 2>&1 || true
run_as_htpc_user kwriteconfig6 --file kwinrc --group Plugins --key "${kwin_script}Enabled" --delete
if [[ -d "$(htpc_user_home)/.local/share/kwin/scripts/${kwin_script}" ]]; then
	rm -rf "$(htpc_user_home)/.local/share/kwin/scripts/${kwin_script}"
	info "Removed the KWin script ${kwin_script}"
fi
