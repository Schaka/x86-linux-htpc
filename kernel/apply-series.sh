#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Applies kernel/patches/series to a kernel source tree.
#
#   kernel/apply-series.sh KERNEL_SOURCE_DIR
#   kernel/apply-series.sh --list      print the checked patch files in order
#
# The script changes the tree. Use a fresh copy of the source, because the
# patches build on each other and a partly patched tree cannot be patched
# again. It stops at the first patch that does not apply, and it names it.
set -euo pipefail

kernel_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
series_file="${kernel_dir}/patches/series"
cache_root="${XDG_CACHE_HOME:-${HOME}/.cache}/x86-linux-htpc"

fail() {
	printf 'ERROR: %s\n' "$*" >&2
	exit 1
}

upstream_value() {
	sed -n "s/^$1=//p" "${kernel_dir}/upstream.conf"
}

# Downloads one upstream file once, and checks it against its checksum
# every time, so a changed cache file cannot slip into a build.
upstream_patch() {
	local path="$1" sha256="$2" repo commit cached raw_url
	repo="$(upstream_value UPSTREAM_REPO)"
	commit="$(upstream_value UPSTREAM_COMMIT)"
	cached="${cache_root}/libreelec-x86-DV/${commit}/${path}"
	if [[ ! -f "${cached}" ]]; then
		raw_url="${repo/github.com/raw.githubusercontent.com}/${commit}/${path}"
		mkdir -p "$(dirname "${cached}")"
		curl -fsSL -o "${cached}.part" "${raw_url}" || fail "Download failed: ${raw_url}"
		mv "${cached}.part" "${cached}"
	fi
	if ! sha256sum --quiet --check <<<"${sha256}  ${cached}" >/dev/null 2>&1; then
		rm -f "${cached}"
		fail "Checksum mismatch for ${path}. The series and the upstream commit do not match."
	fi
	printf '%s\n' "${cached}"
}

apply_patch() {
	local source_dir="$1" patch_file="$2" name="$3"
	if patch -d "${source_dir}" -p1 --forward --no-backup-if-mismatch --quiet <"${patch_file}"; then
		printf 'Applied %s\n' "${name}"
	else
		fail "Patch does not apply: ${name}. See the skill kernel-patches."
	fi
}

# Prints the path of each patch file of the series in order, after the
# download and the checksum test.
series_patch_files() {
	local kind name sha256
	while read -r kind name sha256; do
		case "${kind}" in
		"" | \#*) continue ;;
		upstream) upstream_patch "${name}" "${sha256}" ;;
		local) printf '%s\n' "${kernel_dir}/patches/${name}" ;;
		*) fail "Unknown line in the series: ${kind} ${name}" ;;
		esac
	done <"${series_file}"
}

main() {
	[[ $# -eq 1 ]] || fail "Usage: kernel/apply-series.sh KERNEL_SOURCE_DIR | --list"
	# A command substitution passes the exit status of a failed download or
	# checksum on. A process substitution would hide it.
	local listing
	listing="$(series_patch_files)" || exit 1
	[[ -n "${listing}" ]] || fail "The series has no patches."
	local -a patch_files
	mapfile -t patch_files <<<"${listing}"

	if [[ "$1" == "--list" ]]; then
		printf '%s\n' "${patch_files[@]}"
		return 0
	fi

	local source_dir="$1" patch_file
	[[ -f "${source_dir}/Makefile" && -d "${source_dir}/drivers/gpu/drm" ]] ||
		fail "${source_dir} is not a kernel source tree."
	for patch_file in "${patch_files[@]}"; do
		apply_patch "${source_dir}" "${patch_file}" "$(basename "${patch_file}")"
	done
}

main "$@"
