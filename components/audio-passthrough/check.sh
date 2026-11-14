#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Checks the PipeWire state without playing a sound.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

dropin="$(htpc_user_home)/.config/wireplumber/wireplumber.conf.d/x86-linux-htpc-hdmi.conf"

# Prints the properties of the HDMI sink of PipeWire as KEY=VALUE lines.
hdmi_sink_properties() {
	run_as_htpc_user pw-dump 2>/dev/null | python3 -c '
import json, sys
for item in json.load(sys.stdin):
    props = item.get("info", {}).get("props", {})
    if props.get("media.class") == "Audio/Sink" and "hdmi" in props.get("node.name", ""):
        print("node.name=" + props["node.name"])
        # SPA-JSON lists can come without quotes and commas: [ PCM AC3 ].
        codecs = props.get("iec958.codecs", "[]").strip("[] ").replace(",", " ").replace(chr(34), "")
        print("iec958.codecs=" + " ".join(codecs.split()))
        print("channels=" + str(props.get("audio.channels", "")))
        break
'
}

check "The WirePlumber drop-in exists" test -f "${dropin}"

network_speakers_off() {
	! run_as_htpc_user pw-dump 2>/dev/null | grep -q '"node.name": "raop_sink\.'
}

# The value that WirePlumber uses, from all its configuration files
players_keep_playing() {
	run_as_htpc_user wpctl settings linking.pause-playback 2>/dev/null | grep -qx 'Value: false'
}

if ! run_as_htpc_user systemctl --user is-active --quiet pipewire.service 2>/dev/null; then
	warn "PipeWire of the HTPC user does not run, so the checks of the HDMI sink are skipped."
	checks_done
fi

# After a restart of WirePlumber, the sink appears within a few seconds.
properties=""
for _ in 1 2 3 4 5 6 7 8 9 10; do
	properties="$(hdmi_sink_properties)"
	[[ -n "${properties}" ]] && break
	sleep 1
done
if [[ -z "${properties}" ]]; then
	check_fail "PipeWire has an HDMI sink"
	checks_done
fi
check_pass "PipeWire has an HDMI sink: $(sed -n 's/^node.name=//p' <<<"${properties}")"
check "PipeWire has no AirPlay speakers as sinks" network_speakers_off
check "WirePlumber keeps the players playing when the HDMI output is lost for a moment" players_keep_playing

codecs=" $(sed -n 's/^iec958.codecs=//p' <<<"${properties}") "
for format in $(config_get AUDIO_PASSTHROUGH_FORMATS "ac3 eac3 truehd dts dts-hd"); do
	case "${format}" in
	ac3) name=AC3 ;; eac3) name=EAC3 ;; truehd) name=TrueHD ;; dts) name=DTS ;; dts-hd) name=DTS-HD ;;
	esac
	if [[ "${codecs}" == *" ${name} "* ]]; then
		check_pass "The HDMI sink passes ${name} unchanged"
	else
		check_fail "The HDMI sink does not pass ${name}"
	fi
done

channels="$(sed -n 's/^channels=//p' <<<"${properties}")"
if [[ "${channels}" -ge 6 ]]; then
	check_pass "The HDMI sink has ${channels} channels for decoded audio"
else
	warn "The HDMI sink has ${channels} channels. The receiver gets decoded audio as stereo."
fi

checks_done
