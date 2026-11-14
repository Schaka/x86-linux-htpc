#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Installs the Android apps of ANDROID_APPS into Waydroid, from the GitHub
# releases of each app. files/apps.conf lists the apps. A new release
# replaces the installed app at the next run.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
download_dir=/var/cache/x86-linux-htpc/android-apps
launcher=/usr/local/libexec/x86-linux-htpc/android-app
# The name of the last installed APK of each app, so that a second run with
# the same release changes nothing
state_dir=".local/state/x86-linux-htpc/android-apps"

app_line() {
	read_package_list "${component_dir}/files/apps.conf" | awk -v name="$1" '$1 == name'
}

# pm install needs a running Android session. The component waydroid starts
# it with the Plasma session.
start_session() {
	local _
	run_as_htpc_user systemctl --user start waydroid-session.service
	for _ in $(seq 90); do
		[[ "$(lxc-attach -P /var/lib/waydroid/lxc -n waydroid --clear-env -- /system/bin/getprop sys.boot_completed 2>/dev/null)" == 1 ]] && return 0
		sleep 2
	done
	fail "The Android session did not start."
}

# Waydroid makes desktop entries only for apps with a phone launcher icon.
# TV apps have none, so the component writes an entry for each app. It uses
# the icon that Waydroid exports for each app.
install_desktop_entry() {
	local name="$1" package="$2" title="$3" generated
	generated="$(mktemp)"
	cat >"${generated}" <<ENTRY
[Desktop Entry]
Type=Application
Name=${title}
Comment=Android app
Exec=${launcher} ${package}
Icon=$(htpc_user_home)/.local/share/waydroid/data/icons/${package}.png
Categories=AudioVideo;Video;TV;
ENTRY
	install_user_file "${generated}" ".local/share/applications/htpc-android-${name}.desktop"
	rm -f "${generated}"
}

install_app() {
	local name="$1" line package repository pattern title apk state
	line="$(app_line "${name}")"
	[[ -n "${line}" ]] || fail "ANDROID_APPS names ${name}, but files/apps.conf has no such app."
	read -r _ package repository pattern title <<<"${line}"
	install_desktop_entry "${name}" "${package}" "${title}"
	apk="$(python3 -I "${component_dir}/files/fetch-apk.py" "${repository}" "${pattern}" "${download_dir}")"
	state="$(htpc_user_home)/${state_dir}/${name}"
	if [[ "$(cat "${state}" 2>/dev/null)" == "$(basename "${apk}")" ]] &&
		lxc-attach -P /var/lib/waydroid/lxc -n waydroid --clear-env -- /system/bin/pm path "${package}" >/dev/null 2>&1; then
		return 0
	fi
	# "waydroid app install" ends with success also when Android installs
	# nothing. The package manager of Android reports the real result.
	install -m 0644 "${apk}" "$(htpc_user_home)/.local/share/waydroid/data/local/tmp/htpc-app.apk"
	lxc-attach -P /var/lib/waydroid/lxc -n waydroid --clear-env -- \
		/system/bin/pm install -r /data/local/tmp/htpc-app.apk | grep -qx Success ||
		fail "Android did not install $(basename "${apk}")."
	rm -f "$(htpc_user_home)/.local/share/waydroid/data/local/tmp/htpc-app.apk"
	runuser -u "$(htpc_user)" -- mkdir -p "$(dirname "${state}")"
	basename "${apk}" | runuser -u "$(htpc_user)" -- tee "${state}" >/dev/null
	info "Installed ${name} from $(basename "${apk}")"
}

android() {
	lxc-attach -P /var/lib/waydroid/lxc -n waydroid --clear-env -- "$@"
}

# Twitch puts its ads into the stream itself. S0undTV can load each stream
# through a proxy server in a country without Twitch ads. The proxy URL is
# the setting "Custom server 1", and the value 1 of the active proxy server
# selects it.
configure_s0undtv() {
	local package=com.s0und.s0undtv proxy
	proxy="$(config_get TWITCH_PROXY_URL)"
	[[ -n "${proxy}" ]] || return 0
	# A running app would write its old settings back.
	android /system/bin/am force-stop "${package}"
	python3 -I "${component_dir}/files/android-prefs.py" \
		"$(htpc_user_home)/.local/share/waydroid/data/data/${package}/shared_prefs/${package}_preferences.xml" \
		"pref_proxy_server_custom1_url=${proxy}" pref_proxy_server_name=1
}

# An app that the component installed earlier, and that ANDROID_APPS no
# longer lists, leaves Android and the home screen.
remove_unlisted_apps() {
	local state name line package
	for state in "$(htpc_user_home)/${state_dir}"/*; do
		[[ -f "${state}" ]] || continue
		name="$(basename "${state}")"
		[[ " ${apps} " == *" ${name} "* ]] && continue
		line="$(app_line "${name}")"
		read -r _ package _ <<<"${line}"
		if [[ -n "${package}" ]]; then
			android /system/bin/pm uninstall "${package}" >/dev/null 2>&1 || true
		fi
		rm -f "$(htpc_user_home)/.local/share/applications/htpc-android-${name}.desktop" "${state}"
		info "Removed ${name}, because ANDROID_APPS does not list it"
	done
}

apps="$(config_get ANDROID_APPS)"
if [[ -z "${apps}" ]] && ! compgen -G "$(htpc_user_home)/${state_dir}/*" >/dev/null; then
	info "ANDROID_APPS is empty. No app is installed."
	exit 0
fi
install -d -m 0755 "${download_dir}"
install_file "${component_dir}/files/android-app" "${launcher}" 0755
start_session
remove_unlisted_apps
for name in ${apps}; do
	install_app "${name}"
	if [[ "${name}" == s0undtv ]]; then
		configure_s0undtv
	fi
done
