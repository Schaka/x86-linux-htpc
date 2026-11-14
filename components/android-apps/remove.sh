#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Forgets the installed versions and deletes the downloads. The apps stay
# in Android. Remove them in the Android settings or with
# "waydroid app remove PACKAGE".
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

rm -rf "$(htpc_user_home)/.local/state/x86-linux-htpc/android-apps" /var/cache/x86-linux-htpc/android-apps
rm -f "$(htpc_user_home)"/.local/share/applications/htpc-android-*.desktop
rm -rf "$(htpc_user_home)/.local/share/x86-linux-htpc/icons"
remove_file /usr/local/libexec/x86-linux-htpc/android-app
info "Removed the downloads and the version records of the Android apps"
