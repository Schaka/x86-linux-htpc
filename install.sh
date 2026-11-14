#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Installs and checks the components of the HTPC.
#
#   ./install.sh                  install the components of COMPONENTS in the
#                                 configuration file, or all of them
#   ./install.sh codecs cec       install only these components, then check them
#   ./install.sh --check [names]  only run the checks
#   ./install.sh --remove names   remove these components
set -euo pipefail

# shellcheck source=lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/htpc.sh"

usage() {
	sed -n '4,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

all_components() {
	read_package_list "${HTPC_REPO_ROOT}/components/order.txt"
}

# The components of COMPONENTS, in the order of components/order.txt.
# setup.sh writes the list. Empty: all components.
configured_components() {
	local selected name
	selected=" $(config_get COMPONENTS) "
	if [[ "${selected}" == "  " ]]; then
		all_components
		return 0
	fi
	for name in ${selected}; do
		[[ -d "${HTPC_REPO_ROOT}/components/${name}" ]] || fail "COMPONENTS names the unknown component ${name}."
	done
	while read -r name; do
		[[ "${selected}" == *" ${name} "* ]] && printf '%s\n' "${name}"
	done < <(all_components)
}

component_dir() {
	local dir="${HTPC_REPO_ROOT}/components/$1"
	[[ -d "${dir}" ]] || fail "Unknown component: $1"
	printf '%s\n' "${dir}"
}

main() {
	local action="install"
	local -a names=()
	while [[ $# -gt 0 ]]; do
		case "$1" in
		--check) action="check" ;;
		--remove) action="remove" ;;
		-h | --help)
			usage
			return 0
			;;
		-*) fail "Unknown option: $1" ;;
		*) names+=("$1") ;;
		esac
		shift
	done

	if [[ "${#names[@]}" -eq 0 ]]; then
		[[ "${action}" != "remove" ]] || fail "Name the components to remove."
		mapfile -t names < <(configured_components)
	fi

	local name dir failed=()
	for name in "${names[@]}"; do
		dir="$(component_dir "${name}")"
		info "== ${name}"
		case "${action}" in
		install)
			require_root
			bash "${dir}/install.sh"
			bash "${dir}/check.sh" || failed+=("${name}")
			;;
		check) bash "${dir}/check.sh" || failed+=("${name}") ;;
		remove)
			require_root
			bash "${dir}/remove.sh"
			;;
		esac
	done

	if [[ "${#failed[@]}" -gt 0 ]]; then
		fail "Checks failed for: ${failed[*]}"
	fi
	info "Done."
}

main "$@"
