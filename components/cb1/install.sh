#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Installs the CB1 build: FFmpeg, libplacebo and the CB1 engine with their
# patches, and the CB1 mpv, all in /opt/x86-linux-htpc/cb1. The archive comes
# from cb1/build-in-container.sh. The libraries of the distribution stay.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
prefix=/opt/x86-linux-htpc/cb1

# The SHA256 sum of the unpacked archive stays in the folder, so the same
# archive is not unpacked twice.
install_build() {
	local build_dir archive sum
	build_dir="$(config_require CB1_BUILD_DIR)"
	archive="${build_dir}/x86-linux-htpc-cb1-fc$(fedora_release).tar.gz"
	[[ -f "${archive}" ]] ||
		fail "${archive} is missing. Build it with cb1/build-in-container.sh $(fedora_release) ${build_dir}."
	sum="$(sha256sum "${archive}" | cut -d' ' -f1)"
	if [[ "$(cat "${prefix}/.archive-sha256" 2>/dev/null)" == "${sum}" ]]; then
		return 0
	fi
	rm -rf "${prefix}"
	tar -xzf "${archive}" -C / opt/x86-linux-htpc/cb1
	printf '%s\n' "${sum}" >"${prefix}/.archive-sha256"
	info "Installed the CB1 build from $(basename "${archive}")"
}

install_packages "${component_dir}"
install_build
