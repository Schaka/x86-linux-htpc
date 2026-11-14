#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Prints the newest stable Fedora kernel build of a Fedora release. On
# stderr, it says whether the patch series fits this kernel version, and
# whether a folder already has the patched packages of the build.
#
#   kernel/newest-fedora-kernel.sh [FEDORA_RELEASE [PACKAGES_DIR]]
#   kernel/newest-fedora-kernel.sh 45 ~/htpc-kernel
#
# FEDORA_RELEASE is the release of the running system when it is missing.
# The output on stdout is the argument for kernel/build-fedora.sh.
set -euo pipefail

kernel_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
series_file="${kernel_dir}/patches/series"

fail() {
	printf 'ERROR: %s\n' "$*" >&2
	exit 1
}

info() {
	printf '== %s\n' "$*" >&2
}

# Only the repositories of Fedora count. Another repository of the machine
# can offer a kernel that kernel/build-fedora.sh cannot download as source.
newest_build() {
	local release="$1"
	dnf repoquery --quiet --releasever="${release}" \
		--disablerepo='*' --enablerepo=fedora --enablerepo=updates \
		--latest-limit=1 --queryformat '%{version}-%{release}\n' kernel </dev/null |
		sort -V | tail -n 1
}

# The line "It fits Linux 7.3." in the header of the series.
series_version() {
	sed -n 's/^# It fits Linux \([0-9]\{1,\}\.[0-9]\{1,\}\)\..*$/\1/p' "${series_file}"
}

report_series() {
	local nvr="$1" fits kernel_minor
	fits="$(series_version)"
	[[ -n "${fits}" ]] || fail "The header of ${series_file} has no line 'It fits Linux X.Y.'"
	kernel_minor="$(cut -d. -f1,2 <<<"${nvr%%-*}")"
	if [[ "${kernel_minor}" == "${fits}" ]]; then
		info "The series fits Linux ${fits}, the version of this kernel."
	else
		info "The series fits Linux ${fits}, but this kernel is Linux ${kernel_minor}. Rebase the patches first. See the skill kernel-patches."
	fi
}

# build-fedora.sh puts the tag ".htpc" in front of the Fedora tag of the
# release, for example 300.fc45 becomes 300.htpc.fc45.
report_packages() {
	local nvr="$1" dir="$2" patched_nvr="${1/.fc/.htpc.fc}"
	if compgen -G "${dir}/kernel-core-${patched_nvr}.*.rpm" >/dev/null; then
		info "${dir} has the patched packages of kernel-${nvr}."
	else
		info "${dir} has no patched packages of kernel-${nvr}. Build them with kernel/build-in-container.sh ${nvr} OUTPUT_DIR."
	fi
}

main() {
	[[ $# -le 2 ]] || fail "Usage: kernel/newest-fedora-kernel.sh [FEDORA_RELEASE [PACKAGES_DIR]]"
	local release="${1:-$(rpm -E %fedora)}" packages_dir="${2:-}"
	[[ "${release}" =~ ^[0-9]+$ ]] || fail "FEDORA_RELEASE is a number, for example 45."

	local nvr
	nvr="$(newest_build "${release}")"
	[[ -n "${nvr}" ]] || fail "dnf found no kernel for Fedora ${release}."

	printf '%s\n' "${nvr}"
	report_series "${nvr}"
	if [[ -n "${packages_dir}" ]]; then
		report_packages "${nvr}" "${packages_dir}"
	fi
}

main "$@"
