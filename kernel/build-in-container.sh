#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Runs kernel/build-fedora.sh in a Fedora container, so the build machine
# needs no Fedora of the same release.
#
#   kernel/build-in-container.sh KERNEL_VERSION_RELEASE OUTPUT_DIR
#   kernel/build-in-container.sh 7.3.3-300.fc45 ~/htpc-kernel
#
# The build files stay in ~/.cache/x86-linux-htpc/, so a second run reuses
# the download and the unpacked source.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

fail() {
	printf 'ERROR: %s\n' "$*" >&2
	exit 1
}

main() {
	[[ $# -eq 2 ]] || fail "Usage: kernel/build-in-container.sh KERNEL_VERSION_RELEASE OUTPUT_DIR"
	local nvr="$1" output_dir="$2"
	local fedora_release="${nvr##*.fc}"
	[[ "${fedora_release}" =~ ^[0-9]+$ ]] || fail "${nvr} has no Fedora release, for example 7.3.3-300.fc45."
	command -v podman >/dev/null || fail "This script needs podman."

	local cache="${XDG_CACHE_HOME:-${HOME}/.cache}/x86-linux-htpc"
	mkdir -p "${cache}/container-home" "${output_dir}"
	output_dir="$(cd "${output_dir}" && pwd)"

	podman run --rm \
		--volume "${repo_root}:/repo:ro,z" \
		--volume "${cache}/container-home:/root:z" \
		--volume "${output_dir}:/output:z" \
		"registry.fedoraproject.org/fedora:${fedora_release}" \
		bash -c "dnf install -y rpm-build dnf-plugins-core patch curl git-core && /repo/kernel/build-fedora.sh '${nvr}' /output"
}

main "$@"
