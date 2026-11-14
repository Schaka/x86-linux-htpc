#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Removes the launcher. VacuumTube and its login stay, because the user can
# also use the app without this project.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

home="$(htpc_user_home)"
remove_file "${home}/.local/share/applications/htpc-youtube-tv.desktop"
remove_file "${home}/.var/app/rocks.shy.VacuumTube/config/VacuumTube/flags.txt"
remove_file /usr/local/libexec/x86-linux-htpc/youtube-tv
