#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Selects the HDMI audio driver from AUDIO_DRIVER in the configuration file.
#
#   hda      the standard HD Audio driver (snd-hda-intel), also on machines
#            that use SOF by default
#   default  the choice of the kernel
#
# SOF with IPC3 offers only 2 channels at 48 kHz on HDMI, and its DSP changes
# the samples, so no compressed format reaches the receiver unchanged. The
# HD Audio driver passes all formats, also TrueHD in the HBR mode. With it,
# the internal speakers and microphones of many laptops and Chromebooks stop
# working, but an HTPC does not use them.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

option_file=/etc/modprobe.d/x86-linux-htpc-audio.conf

case "$(config_get AUDIO_DRIVER default)" in
hda)
	generated="$(mktemp)"
	printf '%s\n' "# Written by the component audio-driver of x86-linux-htpc." \
		"options snd_intel_dspcfg dsp_driver=1" >"${generated}"
	install_file "${generated}" "${option_file}"
	rm -f "${generated}"
	;;
default) remove_file "${option_file}" ;;
*) fail "AUDIO_DRIVER must be hda or default." ;;
esac

if [[ "$(config_get AUDIO_DRIVER default)" == hda ]] &&
	[[ "$(basename "$(readlink -f /sys/class/sound/card0/device/driver 2>/dev/null)")" != snd_hda_intel ]]; then
	info "Restart the machine to load the selected audio driver."
fi
