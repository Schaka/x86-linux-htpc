#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Installs htpc-control, the REST API and DLNA renderer of the HTPC, as a
# user service of the Plasma session. docs/control-api.md describes it.
#
# The program comes from CONTROL_BUILD_DIR when control/build.sh put it
# there. Otherwise the install downloads the static build of the newest
# release of the project from GitHub and checks its SHA-256 sum.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
program=/usr/local/libexec/x86-linux-htpc/htpc-control
release_url=https://github.com/Schaka/x86-linux-htpc/releases/latest/download
asset=htpc-control-x86_64-linux

check_settings() {
	local port
	port="$(config_get CONTROL_API_PORT 8780)"
	[[ "${port}" =~ ^[0-9]+$ && "${port}" -ge 1024 && "${port}" -le 65535 ]] ||
		fail "CONTROL_API_PORT must be a port from 1024 to 65535, not ${port}."
	case "$(config_get CONTROL_DLNA yes)" in
	yes | no) ;;
	*) fail "CONTROL_DLNA must be yes or no." ;;
	esac
}

# Prints the path of the program to install. A download goes into the
# folder of the first argument.
fetch_program() {
	local download="$1" local_build
	local_build="$(config_get CONTROL_BUILD_DIR /var/tmp/htpc-control)/htpc-control"
	if [[ -x "${local_build}" ]]; then
		printf '%s\n' "${local_build}"
		return 0
	fi
	curl -fsSL -o "${download}/${asset}" "${release_url}/${asset}" ||
		fail "Cannot download ${asset} from the GitHub release. Build it with control/build.sh instead."
	curl -fsSL -o "${download}/${asset}.sha256" "${release_url}/${asset}.sha256" ||
		fail "Cannot download the SHA-256 sum of ${asset}."
	(cd "${download}" && sha256sum --quiet -c "${asset}.sha256") ||
		fail "The download of ${asset} does not match its SHA-256 sum."
	chmod 0755 "${download}/${asset}"
	printf '%s\n' "${download}/${asset}"
}

# Fedora turns on firewalld. Its desktop zones allow the high ports, but
# other zones block them. A rule for the port and for SSDP lets Home
# Assistant and the DLNA apps reach the service. Each rule goes into the
# running and the permanent configuration. A reload of firewalld would drop
# the runtime rules of other programs, for example the bridge of Waydroid.
open_firewall() {
	local port zone
	command -v firewall-cmd >/dev/null 2>&1 && firewall-cmd --state >/dev/null 2>&1 || return 0
	port="$(config_get CONTROL_API_PORT 8780)"
	zone="$(firewall-cmd --get-default-zone)"
	firewall_allow "${zone}" "--add-port=${port}/tcp" "--query-port=${port}/tcp" "TCP port ${port}"
	if [[ "$(config_get CONTROL_DLNA yes)" == yes ]]; then
		firewall_allow "${zone}" --add-service=ssdp --query-service=ssdp SSDP
	fi
}

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

install_packages "${component_dir}"
check_settings
download_dir="$(mktemp -d)"
trap 'rm -rf "${download_dir}"' EXIT
source_program="$(fetch_program "${download_dir}")"
install_file "${source_program}" "${program}" 0755
enable_user_service "${component_dir}/files/htpc-control.service"
open_firewall
# The service reads the program and the configuration file at its start.
# The restart works only while the user is logged in. Otherwise the
# service starts with the next login.
run_as_htpc_user systemctl --user restart htpc-control.service 2>/dev/null || true
