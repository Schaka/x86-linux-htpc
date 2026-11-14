#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

# The driver of the device behind the first sound card, for example
# snd_hda_intel or sof-audio-pci-intel-tgl. The PCI class of the audio
# controller differs between platforms, so the sound card is the safer start.
audio_controller_driver() {
	local driver
	driver="$(readlink -f /sys/class/sound/card0/device/driver 2>/dev/null)" || return 1
	[[ -n "${driver}" ]] || return 1
	basename "${driver}"
}

hdmi_passthrough_device_exists() {
	local devices
	devices="$(aplay -L 2>/dev/null)"
	grep -q '^hdmi:' <<<"${devices}"
}

wanted="$(config_get AUDIO_DRIVER default)"
driver="$(audio_controller_driver || echo none)"

if [[ "${wanted}" == hda ]]; then
	if [[ "${driver}" == snd_hda_intel ]]; then
		check_pass "The HD Audio driver serves the audio controller"
	else
		check_fail "The audio controller uses ${driver}, not snd_hda_intel. Restart the machine after the install."
	fi
fi
check "ALSA has an HDMI passthrough device (hdmi:)" hdmi_passthrough_device_exists

checks_done
