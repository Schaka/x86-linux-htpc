#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Shared functions for install.sh and the component scripts.
# Source this file. It does not run anything by itself.

# shellcheck disable=SC2034 # install.sh and the components use it
HTPC_REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HTPC_CONFIG="${HTPC_CONFIG:-/etc/x86-linux-htpc/htpc.conf}"

# Messages

info() {
	printf '%s\n' "$*"
}

warn() {
	printf 'WARNING: %s\n' "$*" >&2
}

fail() {
	printf 'ERROR: %s\n' "$*" >&2
	exit 1
}

require_root() {
	[[ "${EUID}" -eq 0 ]] || fail "Run this script as root."
}

# Distribution

# Prints the ID of the distribution, then each ID of ID_LIKE, one per line.
# A component picks the first ID that has a package list.
distribution_ids() {
	local id="" id_like=""
	# shellcheck disable=SC1091 # os-release exists on every systemd distribution
	id="$(. /etc/os-release && printf '%s' "${ID:-}")"
	# shellcheck disable=SC1091
	id_like="$(. /etc/os-release && printf '%s' "${ID_LIKE:-}")"
	local -a like_ids
	read -ra like_ids <<<"${id_like}"
	printf '%s\n' "${id}" "${like_ids[@]}"
}

# Prints the first distribution ID that this library supports.
distribution_family() {
	local id
	while read -r id; do
		case "${id}" in
		fedora)
			printf '%s\n' "${id}"
			return 0
			;;
		esac
	done < <(distribution_ids)
	fail "This distribution is not supported. See docs/distributions.md."
}

# Prints the path of the package list of a component for this distribution.
package_list_of() {
	local component_dir="$1" id
	while read -r id; do
		if [[ -n "${id}" && -f "${component_dir}/packages/${id}.txt" ]]; then
			printf '%s\n' "${component_dir}/packages/${id}.txt"
			return 0
		fi
	done < <(distribution_ids)
	return 1
}

# Prints the package names of a list file, without comments and empty lines.
read_package_list() {
	sed -e 's/#.*//' -e 's/[[:space:]]*$//' -e '/^$/d' "$1"
}

install_packages() {
	local component_dir="$1" list
	list="$(package_list_of "${component_dir}")" ||
		fail "No package list for this distribution in ${component_dir}/packages/. See docs/distributions.md."

	local -a packages
	mapfile -t packages < <(read_package_list "${list}")
	[[ "${#packages[@]}" -gt 0 ]] || return 0

	case "$(basename "${list}" .txt)" in
	fedora) dnf install -y "${packages[@]}" ;;
	*) fail "install_packages does not know the package manager for ${list}." ;;
	esac
}

# Fedora

fedora_release() {
	rpm -E %fedora
}

# RPM Fusion has the codecs that Fedora leaves out because of patents.
fedora_enable_rpmfusion() {
	local release repo
	release="$(fedora_release)"
	for repo in free nonfree; do
		if ! rpm -q "rpmfusion-${repo}-release" >/dev/null 2>&1; then
			dnf install -y "https://mirrors.rpmfusion.org/${repo}/fedora/rpmfusion-${repo}-release-${release}.noarch.rpm"
		fi
	done
}

# Replaces an installed package with another one. Nothing happens if the
# replacement is already installed. Without the old package, the new one can
# still conflict with the free libraries that the desktop pulled in, for
# example libswscale-free, so the install also allows the removal.
fedora_swap_package() {
	local old="$1" new="$2"
	if rpm -q "${new}" >/dev/null 2>&1; then
		return 0
	fi
	if rpm -q "${old}" >/dev/null 2>&1; then
		dnf swap -y --allowerasing "${old}" "${new}"
	else
		dnf install -y --allowerasing "${new}"
	fi
}

package_installed() {
	case "$(distribution_family)" in
	fedora) rpm -q "$1" >/dev/null 2>&1 ;;
	*) fail "package_installed does not know this distribution." ;;
	esac
}

# Files

# Copies a file to the system and reports the change. A file with the same
# content stays untouched, so a second install run changes nothing.
install_file() {
	local source="$1" target="$2" mode="${3:-0644}"
	if [[ -f "${target}" ]] && cmp -s "${source}" "${target}"; then
		return 0
	fi
	install -D -m "${mode}" "${source}" "${target}"
	info "Installed ${target}"
}

remove_file() {
	local target="$1"
	if [[ -e "${target}" ]]; then
		rm -f "${target}"
		info "Removed ${target}"
	fi
}

# Firewall

# Allows one port or service in the zone, in the running and in the
# permanent configuration. A reload of firewalld would drop the runtime
# rules of other programs, for example the bridge of Waydroid, so the
# function never reloads. The arguments are the zone, the add option, the
# query option and a name for the message, for example
# firewall_allow public --add-port=80/tcp --query-port=80/tcp "port 80/tcp".
firewall_allow() {
	local zone="$1" add="$2" query="$3" name="$4" mode
	for mode in "" --permanent; do
		# shellcheck disable=SC2086 # mode is empty or one option
		if ! firewall-cmd ${mode} --zone="${zone}" "${query}" >/dev/null; then
			firewall-cmd ${mode} --zone="${zone}" "${add}" >/dev/null
			[[ -n "${mode}" ]] && info "Allowed ${name} in the firewall zone ${zone}"
		fi
	done
	return 0
}

# Takes back a rule of firewall_allow. The arguments are the zone, the
# remove option, the query option and a name for the message.
firewall_deny() {
	local zone="$1" remove="$2" query="$3" name="$4" mode
	for mode in "" --permanent; do
		# shellcheck disable=SC2086 # mode is empty or one option
		if firewall-cmd ${mode} --zone="${zone}" "${query}" >/dev/null; then
			firewall-cmd ${mode} --zone="${zone}" "${remove}" >/dev/null
			[[ -n "${mode}" ]] && info "Closed ${name} in the firewall zone ${zone}"
		fi
	done
	return 0
}

# Configuration

# Prints the value of KEY from the configuration file, or the default value.
# The file is parsed, not sourced, so a wrong line cannot run a command.
config_get() {
	local key="$1" default="${2:-}" line value
	if [[ -r "${HTPC_CONFIG}" ]]; then
		line="$(grep -E "^[[:space:]]*${key}=" "${HTPC_CONFIG}" | tail -n 1 || true)"
		if [[ -n "${line}" ]]; then
			value="${line#*=}"
			value="${value%\"}"
			value="${value#\"}"
			printf '%s\n' "${value}"
			return 0
		fi
	fi
	printf '%s\n' "${default}"
}

config_require() {
	local key="$1" value
	value="$(config_get "${key}")"
	[[ -n "${value}" ]] || fail "Set ${key} in ${HTPC_CONFIG}. htpc.conf.example describes it."
	printf '%s\n' "${value}"
}

# The HTPC user

htpc_user() {
	config_require HTPC_USER
}

htpc_user_home() {
	getent passwd "$(htpc_user)" | cut -d: -f6
}

# Copies a file into the home folder of the HTPC user. The copy runs as that
# user, so the new folders and the file belong to the user, not to root.
install_user_file() {
	local source="$1" relative_target="$2" mode="${3:-0644}" target
	target="$(htpc_user_home)/${relative_target}"
	if [[ -f "${target}" ]] && cmp -s "${source}" "${target}"; then
		return 0
	fi
	# The user cannot read a private temporary file of root.
	chmod a+r "${source}"
	runuser -u "$(htpc_user)" -- install -D -m "${mode}" "${source}" "${target}"
	info "Installed ${target}"
}

# Installs a systemd user unit for the HTPC user and enables it for the
# target in its WantedBy line. The symbolic link is what "systemctl --user
# enable" makes, but this works also while the user is not logged in.
enable_user_service() {
	local unit_file="$1" name wanted_by link
	name="$(basename "${unit_file}")"
	wanted_by="$(sed -n 's/^WantedBy=//p' "${unit_file}")"
	[[ -n "${wanted_by}" ]] || fail "${name} has no WantedBy line."
	install_user_file "${unit_file}" ".config/systemd/user/${name}"
	# A running user manager reads a changed unit only after a reload.
	run_as_htpc_user systemctl --user daemon-reload 2>/dev/null || true
	link=".config/systemd/user/${wanted_by}.wants/${name}"
	if [[ ! -L "$(htpc_user_home)/${link}" ]]; then
		runuser -u "$(htpc_user)" -- mkdir -p "$(htpc_user_home)/$(dirname "${link}")"
		runuser -u "$(htpc_user)" -- ln -s "../${name}" "$(htpc_user_home)/${link}"
		info "Enabled ${name} for ${wanted_by}"
	fi
}

disable_user_service() {
	local name="$1" home
	home="$(htpc_user_home)"
	find "${home}/.config/systemd/user" -path "*.wants/${name}" -delete 2>/dev/null || true
	remove_file "${home}/.config/systemd/user/${name}"
}

# Sets keys in a JSON file of the HTPC user and keeps all other keys.
# Each argument is KEY=VALUE, with VALUE in JSON, for example fullscreen=true.
merge_user_json() {
	local relative_target="$1"
	shift
	runuser -u "$(htpc_user)" -- python3 "${HTPC_REPO_ROOT}/lib/merge-json.py" \
		"$(htpc_user_home)/${relative_target}" "$@"
}

# Runs a command as the HTPC user, with the variables that systemctl --user
# and other session tools need to find the session of that user.
run_as_htpc_user() {
	local user uid
	user="$(htpc_user)"
	uid="$(id -u "${user}")" || fail "The user ${user} from HTPC_USER does not exist."
	runuser -u "${user}" -- env \
		XDG_RUNTIME_DIR="/run/user/${uid}" \
		DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${uid}/bus" \
		"$@"
}

# Checks

HTPC_CHECKS_FAILED=0

check_pass() {
	printf 'PASS %s\n' "$*"
}

check_fail() {
	printf 'FAIL %s\n' "$*"
	HTPC_CHECKS_FAILED=1
}

# Runs a command quietly and records PASS or FAIL with the description.
check() {
	local description="$1"
	shift
	if "$@" >/dev/null 2>&1; then
		check_pass "${description}"
	else
		check_fail "${description}"
	fi
}

checks_done() {
	exit "${HTPC_CHECKS_FAILED}"
}
