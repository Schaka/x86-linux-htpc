#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

waydroid_installed() {
	command -v waydroid
}

images_present() {
	[[ -f /var/lib/waydroid/images/system.img && -f /var/lib/waydroid/images/vendor.img ]]
}

container_runs() {
	systemctl is-active --quiet waydroid-container.service
}

selinux_module_loaded() {
	[[ "$(getenforce 2>/dev/null)" == Disabled ]] || semodule -l | grep -qx htpc-waydroid
}

check "Waydroid is installed" waydroid_installed
check "SELinux allows the binder calls of Android" selinux_module_loaded
check "The Android image is in /var/lib/waydroid/images" images_present
session_service_enabled() {
	[[ -L "$(htpc_user_home)/.config/systemd/user/graphical-session.target.wants/waydroid-session.service" ]]
}

android_setup_done() {
	[[ "$(lxc-attach -P /var/lib/waydroid/lxc -n waydroid --clear-env -- /system/bin/settings get secure user_setup_complete 2>/dev/null)" == 1 ]]
}

check "The Waydroid container service runs" container_runs
check "The Android session starts with the Plasma session" session_service_enabled
if run_as_htpc_user waydroid status 2>/dev/null | grep -q 'Session:.*RUNNING'; then
	check "The Android setup wizard is done" android_setup_done
fi

checks_done
