#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Configures PipeWire for audio passthrough on HDMI, through a WirePlumber
# drop-in of the HTPC user.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
dropin=".config/wireplumber/wireplumber.conf.d/x86-linux-htpc-hdmi.conf"

# PipeWire names the codecs differently from mpv and from the configuration
# file. PCM is always in the list.
pipewire_codecs() {
	local format codecs="PCM"
	for format in $(config_get AUDIO_PASSTHROUGH_FORMATS "ac3 eac3 truehd dts dts-hd"); do
		case "${format}" in
		ac3) codecs+=" AC3" ;;
		eac3) codecs+=" EAC3" ;;
		truehd) codecs+=" TrueHD" ;;
		dts) codecs+=" DTS" ;;
		dts-hd) codecs+=" DTS-HD" ;;
		*) fail "Unknown format in AUDIO_PASSTHROUGH_FORMATS: ${format}" ;;
		esac
	done
	printf '%s\n' "${codecs}"
}

suspend_timeout() {
	case "$(config_get AUDIO_KEEP_HDMI_OPEN yes)" in
	yes) printf '0\n' ;;
	no) printf '5\n' ;;
	*) fail "AUDIO_KEEP_HDMI_OPEN must be yes or no." ;;
	esac
}

# Fedora adds the AirPlay speakers of the LAN as PipeWire sinks, through the
# package pipewire-config-raop. When the TV turns on or off, the receiver
# sends a hotplug, and the HDMI sink is gone for a moment. PipeWire then
# moves the stream of a player to an AirPlay speaker, maybe in another room,
# and the stream does not come back to HDMI.
remove_network_speakers() {
	[[ "$(distribution_family)" == fedora ]] || return 0
	package_installed pipewire-config-raop || return 0
	dnf remove -y pipewire-config-raop
	# PipeWire reads its configuration only at the start. WirePlumber
	# restarts with it.
	if run_as_htpc_user systemctl --user is-active --quiet pipewire.service; then
		run_as_htpc_user systemctl --user restart pipewire.service
	fi
}

remove_network_speakers

generated="$(mktemp)"
sed -e "s/@CODECS@/$(pipewire_codecs)/" -e "s/@SUSPEND_TIMEOUT@/$(suspend_timeout)/" \
	"${component_dir}/files/x86-linux-htpc-hdmi.conf.in" >"${generated}"
before="$(sha256sum "$(htpc_user_home)/${dropin}" 2>/dev/null || true)"
install_user_file "${generated}" "${dropin}"
rm -f "${generated}"

# WirePlumber stores the volume of each output. The default volume of the
# drop-in reaches only an output without a stored volume, so a changed
# drop-in also sets each HDMI output to the full level once.
full_volume_on_hdmi() {
	local node
	sleep 3
	for node in $(run_as_htpc_user pw-dump 2>/dev/null | python3 -c '
import json, sys
for o in json.load(sys.stdin):
    props = o.get("info", {}).get("props", {}) if isinstance(o.get("info"), dict) else {}
    name = props.get("node.name", "")
    if name.startswith("alsa_output.") and "hdmi" in name:
        print(o["id"])'); do
		run_as_htpc_user wpctl set-volume "${node}" 1.0
	done
}

# WirePlumber reads its configuration only at the start.
if [[ "${before}" != "$(sha256sum "$(htpc_user_home)/${dropin}")" ]] &&
	run_as_htpc_user systemctl --user is-active --quiet wireplumber.service; then
	run_as_htpc_user systemctl --user restart wireplumber.service
	full_volume_on_hdmi
fi
