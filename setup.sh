#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Sets up the HTPC on a fresh install: asks the questions, writes the
# configuration file, builds what is missing and installs the components.
#
#   sudo ./setup.sh                         ask each question on the terminal
#   sudo ./setup.sh --answers FILE          take the answers from FILE
#   sudo ./setup.sh --answers FILE --no-install
#                                           only write the configuration
#   ./setup.sh --questions                  list the questions and their keys
#   ./setup.sh --defaults                   print the default answers for this
#                                           machine, with the detected values
#
# An answers file has KEY=value lines with the keys of --questions. A key
# that the file does not have gets its default. The skill setup-htpc in
# .agents/skills/ describes how an AI agent asks the user and writes the
# file. docs/install.md describes the steps for a person.
set -euo pipefail

# shellcheck source=lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/htpc.sh"

example="${HTPC_REPO_ROOT}/htpc.conf.example"
answers_file=""
interactive=yes
do_install=yes
declare -A answer=()
declare -A given=()
declare -a extra=()

usage() {
	sed -n '4,18p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

print_defaults() {
	local key default
	while IFS='|' read -r key _ default _; do
		[[ "${default}" == @* ]] && default="$("${default#@}")"
		answer[${key}]="${default}"
		printf '%s=%s\n' "${key}" "${default}"
	done < <(questions)
}

# The questions, one per line: KEY|question|default|allowed values. An empty
# list of allowed values takes any text. The default can name a detect_
# function, which the script runs on this machine.
questions() {
	cat <<'LIST'
HTPC_USER|The user that logs in to the TV session automatically. The user must exist.|@detect_user|
HDMI_OUTPUT|The HDMI output of the TV.|@detect_hdmi_output|
DISPLAY_MODE|The output mode of the desktop as WIDTHxHEIGHT@RATE.|@detect_display_mode|
AUDIO_DRIVER|The HDMI audio driver: hda for machines with SOF audio (many laptops and Chromebooks), else default.|@detect_audio_driver|hda default
SETUP_AV_RECEIVER|Is an AV receiver between the HTPC and the TV? yes sends all audio formats unchanged. no sends only Dolby Digital and Dolby Digital Plus, which most TVs accept.|yes|yes no
SETUP_JELLYFIN|Play from a Jellyfin server with jellyfin-mpv-shim?|yes|yes no
JELLYFIN_SERVER_URL|The address of the Jellyfin server, for example http://192.0.2.10:8096.||
JELLYFIN_PLAYER_NAME|The name of the HTPC in the device list of Jellyfin. Empty: the host name.||
SETUP_DOLBY_VISION|Play Dolby Vision with the TV in its Dolby Vision mode? This needs the patched kernel and the CB1 build, an Intel GPU and a TV with Dolby Vision.|no|yes no
SETUP_KODI|Install Kodi with the CB1 engine as a second player? This needs the CB1 build.|no|yes no
SETUP_YOUTUBE|Install YouTube with the TV interface (VacuumTube)?|yes|yes no
ANDROID_APPS|The Android TV apps in Waydroid, separated by spaces: smarttube, s0undtv (Twitch), tizentube. Empty: no Waydroid.|smarttube s0undtv|
TWITCH_PROXY_URL|The proxy server of S0undTV that removes the Twitch ads. Empty: no proxy.|https://eu.luminous.dev/live/${channel}?allow_source=true&allow_audio_only=true&fast_bread=true|
SETUP_REMOTE|The remote: air-mouse for a 2.4 GHz air mouse that sends OK and Back as mouse buttons, keyboard for a remote, keyboard or mouse with normal keys.|keyboard|air-mouse keyboard
AIR_MOUSE_USB_IDS|The USB IDs of the air mouse receivers as VENDOR:PRODUCT, separated by spaces.|@detect_air_mouse|
SCREEN_EDGES|Actions in the screen corners of KWin: off stops the overview that a pointer in a corner opens.|off|on off
WAKE_ON_LAN_MACS|The MAC addresses of the TV for Wake-on-LAN, separated by spaces. Empty: no network wake.||
SETUP_BUILD|Build the missing CB1, Kodi and kernel packages on this machine now? This takes hours. no: put them into CB1_BUILD_DIR and KERNEL_PACKAGES_DIR yourself.|yes|yes no
CB1_BUILD_DIR|The folder for the archives of the CB1 build and of Kodi.|/var/tmp/htpc-cb1|
KERNEL_PACKAGES_DIR|The folder for the packages of the patched kernel.|/var/tmp/htpc-kernel|
SETUP_HOME_ASSISTANT|Does a Home Assistant server control the TV? The setup then shows the guide docs/home-assistant.md.|no|yes no
SETUP_CEC|Is a USB-CEC adapter connected? The setup then shows the guide docs/cec.md.|no|yes no
LIST
}

# Detection. Each function prints a default for this machine. The functions
# read sysfs, which every Linux distribution has, and need no package.

detect_user() {
	if [[ -n "${SUDO_USER:-}" && "${SUDO_USER}" != root ]]; then
		printf '%s\n' "${SUDO_USER}"
	else
		getent passwd | awk -F: '$3 >= 1000 && $3 < 60000 {print $1; exit}'
	fi
}

detect_hdmi_output() {
	local status name first=""
	for status in /sys/class/drm/card*-HDMI-A-*/status; do
		[[ -f "${status}" ]] || continue
		name="$(basename "$(dirname "${status}")")"
		name="${name#card*-}"
		[[ -n "${first}" ]] || first="${name}"
		if [[ "$(cat "${status}")" == connected ]]; then
			printf '%s\n' "${name}"
			return 0
		fi
	done
	printf '%s\n' "${first:-HDMI-A-1}"
}

# sysfs lists the modes of the EDID with the preferred mode first, but
# without the refresh rate. 60 Hz fits each TV.
detect_display_mode() {
	local modes size=""
	modes="$(compgen -G "/sys/class/drm/card*-${answer[HDMI_OUTPUT]:-HDMI-A-1}/modes" | head -n 1 || true)"
	[[ -n "${modes}" ]] && size="$(head -n 1 "${modes}" 2>/dev/null || true)"
	printf '%s@60\n' "${size:-3840x2160}"
}

# The audio device of the PCI bus has the SOF driver in front. Other
# machines load the SOF modules too, but use snd_hda_intel.
detect_audio_driver() {
	local driver
	for driver in /sys/bus/pci/devices/*/driver; do
		if [[ "$(basename "$(readlink -f "${driver}")")" == sof-audio-pci* ]]; then
			printf 'hda\n'
			return 0
		fi
	done
	printf 'default\n'
}

# An air mouse receiver is one USB device with a keyboard and a mouse
# interface (HID boot protocols 1 and 2). Many wireless keyboards look the
# same, so the user confirms the list.
detect_air_mouse() {
	local device ids=""
	for device in /sys/bus/usb/devices/*; do
		[[ -f "${device}/idVendor" ]] || continue
		if grep -qx 01 "${device}"/*/bInterfaceProtocol 2>/dev/null &&
			grep -qx 02 "${device}"/*/bInterfaceProtocol 2>/dev/null; then
			ids+="$(cat "${device}/idVendor"):$(cat "${device}/idProduct") "
		fi
	done
	printf '%s\n' "${ids% }"
}

describe_usb_devices() {
	local device
	for device in /sys/bus/usb/devices/*; do
		[[ -f "${device}/idVendor" && -f "${device}/product" ]] || continue
		printf '  %s:%s  %s\n' "$(cat "${device}/idVendor")" "$(cat "${device}/idProduct")" "$(cat "${device}/product")"
	done
}

# Questions that do not apply to the earlier answers are skipped.
applies() {
	case "$1" in
	JELLYFIN_SERVER_URL | JELLYFIN_PLAYER_NAME) [[ "${answer[SETUP_JELLYFIN]}" == yes ]] ;;
	TWITCH_PROXY_URL) [[ " ${answer[ANDROID_APPS]} " == *" s0undtv "* ]] ;;
	AIR_MOUSE_USB_IDS) [[ "${answer[SETUP_REMOTE]}" == air-mouse ]] ;;
	SETUP_BUILD | CB1_BUILD_DIR | KERNEL_PACKAGES_DIR)
		[[ "${answer[SETUP_DOLBY_VISION]}" == yes || "${answer[SETUP_KODI]}" == yes ]]
		;;
	*) true ;;
	esac
}

valid() {
	local key="$1" value="$2" allowed="$3" item
	if [[ -n "${allowed}" && " ${allowed} " != *" ${value} "* ]]; then
		printf 'Answer one of: %s\n' "${allowed}" >&2
		return 1
	fi
	case "${key}" in
	HTPC_USER)
		getent passwd "${value}" >/dev/null || {
			printf 'The user %s does not exist. Create it first, for example with useradd -m.\n' "${value}" >&2
			return 1
		}
		;;
	DISPLAY_MODE)
		[[ "${value}" =~ ^[0-9]+x[0-9]+@[0-9.]+$ ]] || {
			printf 'Use WIDTHxHEIGHT@RATE, for example 3840x2160@60.\n' >&2
			return 1
		}
		;;
	JELLYFIN_SERVER_URL)
		[[ "${value}" =~ ^https?://[^[:space:]]+$ ]] || {
			printf 'Use an address that starts with http:// or https://.\n' >&2
			return 1
		}
		;;
	ANDROID_APPS)
		for item in ${value}; do
			read_package_list "${HTPC_REPO_ROOT}/components/android-apps/files/apps.conf" |
				awk -v name="${item}" '$1 == name {found = 1} END {exit !found}' || {
				printf 'Unknown app: %s\n' "${item}" >&2
				return 1
			}
		done
		;;
	AIR_MOUSE_USB_IDS)
		for item in ${value}; do
			[[ "${item}" =~ ^[0-9a-fA-F]{4}:[0-9a-fA-F]{4}$ ]] || {
				printf 'Use VENDOR:PRODUCT, for example 1234:abcd.\n' >&2
				return 1
			}
		done
		;;
	WAKE_ON_LAN_MACS)
		for item in ${value}; do
			[[ "${item}" =~ ^([0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}$ ]] || {
				printf 'Use the form 00:00:5e:00:53:01.\n' >&2
				return 1
			}
		done
		;;
	esac
}

# On a second run, the choices of the last run follow from COMPONENTS and
# the audio formats of the configuration file.
earlier_choice() {
	local key="$1" installed
	[[ -f "${HTPC_CONFIG}" ]] || return 1
	installed=" $(config_get COMPONENTS) "
	[[ "${installed}" != "  " ]] || return 1
	has() { [[ "${installed}" == *" $1 "* ]] && printf 'yes\n' || printf 'no\n'; }
	case "${key}" in
	SETUP_JELLYFIN) has jellyfin-mpv-shim ;;
	SETUP_DOLBY_VISION) has kernel-dv ;;
	SETUP_KODI) has kodi-dv ;;
	SETUP_YOUTUBE) has youtube-tv ;;
	SETUP_REMOTE) [[ "$(has remote-keys)" == yes ]] && printf 'air-mouse\n' || printf 'keyboard\n' ;;
	SETUP_AV_RECEIVER) [[ " $(config_get AUDIO_PASSTHROUGH_FORMATS) " == *" truehd "* ]] && printf 'yes\n' || printf 'no\n' ;;
	*) return 1 ;;
	esac
}

ask() {
	local key="$1" text="$2" default="$3" allowed="$4" value earlier
	if [[ "${key}" != SETUP_* && -f "${HTPC_CONFIG}" ]] && grep -q "^${key}=" "${HTPC_CONFIG}"; then
		# A second run keeps the values of the last run as defaults.
		default="$(config_get "${key}")"
	elif earlier="$(earlier_choice "${key}")"; then
		default="${earlier}"
	elif [[ "${default}" == @* ]]; then
		default="$("${default#@}")"
	fi
	if [[ "${interactive}" == no ]]; then
		value="${given[${key}]-${default}}"
		valid "${key}" "${value}" "${allowed}" || fail "The answer ${key}=${value} is not valid."
		answer[${key}]="${value}"
		printf '%s=%s\n' "${key}" "${value}"
		return 0
	fi
	printf '\n%s\n' "${text}"
	[[ "${key}" == AIR_MOUSE_USB_IDS ]] && { printf 'USB devices of this machine:\n'; describe_usb_devices; }
	while true; do
		read -r -p "${key} [${default}]: " value
		value="${value:-${default}}"
		valid "${key}" "${value}" "${allowed}" && break
	done
	answer[${key}]="${value}"
}

read_answers() {
	local line key value
	[[ -r "${answers_file}" ]] || fail "Cannot read ${answers_file}."
	while IFS= read -r line || [[ -n "${line}" ]]; do
		[[ "${line}" =~ ^[[:space:]]*(#|$) ]] && continue
		[[ "${line}" =~ ^[[:space:]]*([A-Z][A-Z0-9_]*)=(.*)$ ]] || fail "Not KEY=value in ${answers_file}: ${line}"
		key="${BASH_REMATCH[1]}"
		value="${BASH_REMATCH[2]}"
		value="${value%\"}"
		value="${value#\"}"
		given[${key}]="${value}"
	done <"${answers_file}"
	for key in "${!given[@]}"; do
		questions | cut -d'|' -f1 | grep -qx "${key}" && continue
		# Other keys of the configuration file go into it unchanged.
		grep -q "^${key}=" "${example}" ||
			fail "${answers_file} has the unknown key ${key}. See ./setup.sh --questions and htpc.conf.example."
		extra+=("${key}=${given[${key}]}")
	done
}

ask_all() {
	local key text default allowed
	while IFS='|' read -r key text default allowed; do
		applies "${key}" || continue
		ask "${key}" "${text}" "${default}" "${allowed}"
	done < <(questions)
	# A skipped question keeps an empty answer, so later steps can read it.
	while IFS='|' read -r key _; do
		[[ -v "answer[${key}]" ]] || answer[${key}]=""
	done < <(questions)
}

# The browser on the home screen, if the system has one
browser_entry() {
	local entry
	for entry in org.mozilla.firefox.desktop firefox.desktop; do
		if [[ -f "/usr/share/applications/${entry}" || -f "/var/lib/flatpak/exports/share/applications/${entry}" ]]; then
			printf '%s\n' "${entry}"
			return 0
		fi
	done
}

# The components and the home screen follow from the answers.
plan_install() {
	local -a list=(codecs bigscreen audio-driver audio-passthrough)
	local -a apps=()
	local app
	if [[ "${answer[SETUP_DOLBY_VISION]}" == yes ]]; then
		list+=(kernel-dv cb1)
	elif [[ "${answer[SETUP_KODI]}" == yes ]]; then
		list+=(cb1)
	fi
	if [[ "${answer[SETUP_JELLYFIN]}" == yes ]]; then
		list+=(jellyfin-mpv-shim)
		apps+=(com.github.iwalton3.jellyfin-mpv-shim.desktop)
	fi
	[[ "${answer[SETUP_YOUTUBE]}" == yes ]] && { list+=(youtube-tv); apps+=(htpc-youtube-tv.desktop); }
	if [[ -n "${answer[ANDROID_APPS]}" ]]; then
		list+=(waydroid android-apps)
		for app in ${answer[ANDROID_APPS]}; do
			apps+=("htpc-android-${app}.desktop")
		done
	fi
	[[ "${answer[SETUP_KODI]}" == yes ]] && { list+=(kodi-dv); apps+=(htpc-kodi.desktop); }
	[[ "${answer[SETUP_REMOTE]}" == air-mouse && -n "${answer[AIR_MOUSE_USB_IDS]}" ]] && list+=(remote-keys)
	[[ -n "${answer[WAKE_ON_LAN_MACS]}" ]] && list+=(network-wake)
	app="$(browser_entry || true)"
	[[ -n "${app}" ]] && apps+=("${app}")
	list+=(home-screen)
	components="${list[*]}"
	home_apps="${apps[*]}"
}

# A minimal Fedora image has no Python, and the script that writes the
# configuration file needs it.
ensure_python() {
	command -v python3 >/dev/null 2>&1 && return 0
	install_packages "${HTPC_REPO_ROOT}/setup/base"
}

write_config() {
	local formats="ac3 eac3 truehd dts dts-hd" mpv=system
	[[ "${answer[SETUP_AV_RECEIVER]}" == no ]] && formats="ac3 eac3"
	[[ "${answer[SETUP_DOLBY_VISION]}" == yes ]] && mpv=cb1
	local -a pairs=(
		"COMPONENTS=${components}"
		"HOME_SCREEN_APPS=${home_apps}"
		"AUDIO_PASSTHROUGH_FORMATS=${formats}"
		"JELLYFIN_MPV=${mpv}"
	)
	local key
	ensure_python
	while IFS='|' read -r key _; do
		[[ "${key}" == SETUP_* ]] && continue
		# A skipped question keeps the value of the example file.
		applies "${key}" || continue
		pairs+=("${key}=${answer[${key}]}")
	done < <(questions)
	python3 -I "${HTPC_REPO_ROOT}/lib/set-config.py" "${HTPC_CONFIG}" "${example}" "${pairs[@]}" "${extra[@]}"
	info "Wrote ${HTPC_CONFIG}"
}

# The CB1 build, Kodi and the kernel are large builds. The scripts run in
# containers of the Fedora release of this machine.
build_missing() {
	local release cb1_dir kernel_dir need_cb1=no need_kodi=no nvr
	[[ "$(distribution_family)" == fedora ]] ||
		fail "The builds of CB1, Kodi and the kernel exist only for Fedora. See docs/distributions.md."
	release="$(fedora_release)"
	cb1_dir="${answer[CB1_BUILD_DIR]}"
	kernel_dir="${answer[KERNEL_PACKAGES_DIR]}"
	[[ " ${components} " == *" cb1 "* && ! -f "${cb1_dir}/x86-linux-htpc-cb1-fc${release}.tar.gz" ]] && need_cb1=yes
	[[ " ${components} " == *" kodi-dv "* && ! -f "${cb1_dir}/x86-linux-htpc-kodi-fc${release}.tar.gz" ]] && need_kodi=yes
	if [[ "${need_cb1}" == yes || "${need_kodi}" == yes ]]; then
		install_packages "${HTPC_REPO_ROOT}/setup/builds"
		if [[ "${need_kodi}" == yes ]]; then
			"${HTPC_REPO_ROOT}/cb1/build-in-container.sh" "${release}" "${cb1_dir}"
		else
			"${HTPC_REPO_ROOT}/cb1/build-in-container.sh" --no-kodi "${release}" "${cb1_dir}"
		fi
	fi
	if [[ " ${components} " == *" kernel-dv "* ]] && ! compgen -G "${kernel_dir}/kernel-core-*.htpc.*.rpm" >/dev/null; then
		install_packages "${HTPC_REPO_ROOT}/setup/builds"
		nvr="$("${HTPC_REPO_ROOT}/kernel/newest-fedora-kernel.sh" "${release}")"
		"${HTPC_REPO_ROOT}/kernel/build-in-container.sh" "${nvr}" "${kernel_dir}"
	fi
}

next_steps() {
	printf '\n== Next steps\n'
	if [[ "${answer[SETUP_JELLYFIN]}" == yes ]]; then
		printf '1. Log the HTPC in to Jellyfin: sudo components/jellyfin-mpv-shim/login.sh\n'
		printf '   It shows a code. Approve it in another Jellyfin client: user menu, Quick Connect.\n'
	fi
	printf '2. Restart the computer. The TV then shows Plasma Bigscreen.\n'
	printf '   Then run sudo ./install.sh home-screen. It needs the running session to hide the rows and to bind the Home key.\n'
	[[ " ${components} " == *" kernel-dv "* ]] &&
		printf '3. The patched kernel starts once. If the system works, run sudo ./install.sh kernel-dv to keep it.\n'
	[[ "${answer[SETUP_HOME_ASSISTANT]}" == yes ]] &&
		printf -- '- Home Assistant: read docs/home-assistant.md.\n'
	[[ "${answer[SETUP_CEC]}" == yes ]] &&
		printf -- '- CEC: the project has no CEC component yet. Read docs/cec.md.\n'
	[[ -n "${answer[ANDROID_APPS]}" ]] &&
		printf -- '- Sign in to the Android apps and to YouTube on the TV.\n'
	return 0
}

main() {
	while [[ $# -gt 0 ]]; do
		case "$1" in
		--answers)
			[[ $# -ge 2 ]] || fail "--answers needs a file."
			answers_file="$2"
			interactive=no
			shift
			;;
		--no-install) do_install=no ;;
		--questions)
			questions | awk -F'|' '{
				dflt = ($3 ~ /^@/) ? "detected on this machine" : ($3 == "" ? "empty" : $3)
				printf "%s\n    %s\n    Default: %s. Values: %s.\n", $1, $2, dflt, ($4 == "" ? "any text" : $4)
			}'
			return 0
			;;
		--defaults)
			print_defaults
			return 0
			;;
		-h | --help)
			usage
			return 0
			;;
		*) fail "Unknown argument: $1. See ./setup.sh --help." ;;
		esac
		shift
	done
	require_root
	# The check stops on a distribution without support, before a question.
	info "Distribution: $(distribution_family)"
	[[ -n "${answers_file}" ]] && read_answers

	ask_all
	plan_install
	info "Components: ${components}"
	info "Home screen: ${home_apps}"
	write_config
	[[ "${do_install}" == yes ]] || return 0

	if [[ "${answer[SETUP_DOLBY_VISION]}" == yes || "${answer[SETUP_KODI]}" == yes ]] &&
		[[ "${answer[SETUP_BUILD]}" == yes ]]; then
		build_missing
	fi
	# On a fresh machine some checks fail until the Jellyfin login and the
	# first Plasma session exist. The next steps tell the user what to do, so
	# they print even then.
	local status=0
	"${HTPC_REPO_ROOT}/install.sh" || status=$?
	next_steps
	return "${status}"
}

main "$@"
