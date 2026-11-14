#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Removes the automatic login. The packages and the options of the user
# stay, because the user can also use them without this project.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

remove_file /etc/plasmalogin.conf.d/x86-linux-htpc.conf
remove_file /etc/sddm.conf.d/x86-linux-htpc.conf
remove_file /etc/systemd/logind.conf.d/x86-linux-htpc.conf
remove_file "$(htpc_user_home)/.config/environment.d/x86-linux-htpc-kwin.conf"
disable_user_service htpc-display.service
