#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

state_dir="$(htpc_user_home)/.local/state/x86-linux-htpc/android-apps"

app_installed() {
	[[ -s "${state_dir}/$1" ]]
}

entry_present() {
	[[ -f "$(htpc_user_home)/.local/share/applications/htpc-android-$1.desktop" ]]
}

# The settings file of the app has the proxy URL as XML text, so "&"
# appears as "&amp;".
s0undtv_proxy_set() {
	local package=com.s0und.s0undtv proxy
	proxy="$(config_get TWITCH_PROXY_URL)"
	grep -qF "<string name=\"pref_proxy_server_custom1_url\">${proxy//'&'/'&amp;'}</string>" \
		"$(htpc_user_home)/.local/share/waydroid/data/data/${package}/shared_prefs/${package}_preferences.xml" 2>/dev/null
}

for name in $(config_get ANDROID_APPS); do
	check "The Android app ${name} is installed" app_installed "${name}"
	check "The Android app ${name} has a desktop entry" entry_present "${name}"
	if [[ "${name}" == s0undtv && -n "$(config_get TWITCH_PROXY_URL)" ]]; then
		check "S0undTV loads Twitch streams through TWITCH_PROXY_URL" s0undtv_proxy_set
	fi
done

checks_done
