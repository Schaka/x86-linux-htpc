#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Installs Waydroid with an Android TV image and starts its container at
# boot. WAYDROID_SYSTEM_OTA and WAYDROID_VENDOR_OTA in the configuration
# file name the image. docs/android-apps.md explains the choice.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
images_dir=/var/lib/waydroid/images

# waydroid init downloads the images from the OTA channels and compares
# each download with the SHA256 sum that the channel lists.
init_waydroid() {
	local system_ota vendor_ota
	system_ota="$(config_get WAYDROID_SYSTEM_OTA https://waydroid-atv.github.io/ota/a16-tv/system)"
	vendor_ota="$(config_get WAYDROID_VENDOR_OTA https://waydroid-atv.github.io/ota/a16-tv/vendor)"
	if [[ -f "${images_dir}/system.img" && -f "${images_dir}/vendor.img" ]] &&
		# Waydroid stores the channel with the path of the JSON file below it.
		grep -qF "system_ota = ${system_ota}/" /var/lib/waydroid/waydroid.cfg 2>/dev/null; then
		return 0
	fi
	info "Downloading the Android image. This takes some minutes."
	waydroid init -f -c "${system_ota}" -v "${vendor_ota}" -r lineage -s GAPPS
}

# Fedora runs SELinux. files/htpc-waydroid.te adds the binder rules that
# the policy of waydroid-selinux lacks for the Android TV image.
install_selinux_module() {
	command -v semodule >/dev/null || return 0
	[[ "$(getenforce 2>/dev/null)" != Disabled ]] || return 0
	local work
	work="$(mktemp -d)"
	checkmodule -M -m -o "${work}/htpc-waydroid.mod" "${component_dir}/files/htpc-waydroid.te"
	semodule_package -o "${work}/htpc-waydroid.pp" -m "${work}/htpc-waydroid.mod"
	if ! semodule -l | grep -qx htpc-waydroid ||
		! cmp -s "${work}/htpc-waydroid.pp" /var/lib/x86-linux-htpc/htpc-waydroid.pp; then
		semodule -i "${work}/htpc-waydroid.pp"
		install -D -m 0644 "${work}/htpc-waydroid.pp" /var/lib/x86-linux-htpc/htpc-waydroid.pp
		info "Loaded the SELinux module htpc-waydroid"
	fi
	rm -rf "${work}"
}

# The setup wizard of the Android TV image waits for the pairing of a
# Bluetooth remote. The HTPC sends the keys of its own remote to Android,
# so the wizard skips this step. Android reads the file when the container
# starts.
skip_remote_pairing() {
	local line=atv.setup.bt_remote_pairing=false prop=/var/lib/waydroid/waydroid_base.prop
	grep -qxF "${line}" "${prop}" 2>/dev/null && return 0
	printf '%s\n' "${line}" >>"${prop}"
	systemctl restart waydroid-container.service
	info "Turned off the remote pairing step of the Android setup"
}

# The setup wizard of the Android TV image asks for a language, a network
# and a Google account on the first start. The apps need none of them.
finish_android_setup() {
	local _ attach=(lxc-attach -P /var/lib/waydroid/lxc -n waydroid --clear-env --)
	for _ in $(seq 90); do
		[[ "$("${attach[@]}" /system/bin/getprop sys.boot_completed 2>/dev/null)" == 1 ]] && break
		sleep 2
	done
	[[ "$("${attach[@]}" /system/bin/getprop sys.boot_completed 2>/dev/null)" == 1 ]] ||
		fail "Android did not finish its start."
	"${attach[@]}" /system/bin/settings put global device_provisioned 1
	"${attach[@]}" /system/bin/settings put secure user_setup_complete 1
	"${attach[@]}" /system/bin/settings put secure tv_user_setup_complete 1
	local wizard
	for wizard in com.google.android.tungsten.setupwraith org.lineageos.setupwizard; do
		"${attach[@]}" /system/bin/pm disable-user --user 0 "${wizard}" >/dev/null 2>&1 || true
	done
}

install_packages "${component_dir}"
install_selinux_module
init_waydroid
skip_remote_pairing
enable_user_service "${component_dir}/files/waydroid-session.service"
if run_as_htpc_user systemctl --user is-active --quiet graphical-session.target 2>/dev/null; then
	run_as_htpc_user systemctl --user start waydroid-session.service
	finish_android_setup
else
	warn "The Plasma session does not run. Run the install again after the login, so that it can finish the Android setup."
fi
systemctl enable --now waydroid-container.service >/dev/null
