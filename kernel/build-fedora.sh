#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Builds the Fedora kernel packages with the patch series.
#
#   kernel/build-fedora.sh KERNEL_VERSION_RELEASE [OUTPUT_DIR]
#   kernel/build-fedora.sh 7.3.3-300.fc45 ~/htpc-kernel
#
# Run it on Fedora as root, or in a Fedora container with
# kernel/build-in-container.sh. The Fedora release of the system must match
# the release in KERNEL_VERSION_RELEASE, because dnf downloads the source
# package and the build dependencies from the repositories of that release.
#
# The packages get the release tag ".htpc", for example
# kernel-7.3.3-300.htpc.fc45, so they install next to the stock kernel.
set -euo pipefail

kernel_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
build_id=".htpc"

fail() {
	printf 'ERROR: %s\n' "$*" >&2
	exit 1
}

# Progress messages go to stderr, because some functions print their result
# on stdout.
info() {
	printf '== %s\n' "$*" >&2
}

# The kernel parts that an HTPC does not use. Leaving them out cuts the
# build time by more than half.
rpmbuild_options=(
	--without debug
	--without debuginfo
	--without doc
	--without headers
	--without kabichk
	--without kabidwchk
	--without kernel_abi_stablelists
	--without libperf
	--without perf
	--without realtime
	--without selftests
	--without tools
	--without ynl
	--without zfcpdump
	--without efiuki
	--target x86_64
)

download_source_package() {
	local nvr="$1" topdir="$2"
	local srpm="${topdir}/SRPMS/kernel-${nvr}.src.rpm"
	if [[ ! -f "${srpm}" ]]; then
		info "Downloading kernel-${nvr}.src.rpm"
		# dnf checks the signature of the package with the keys of the system.
		dnf download --source --destdir "${topdir}/SRPMS" "kernel-${nvr}" >&2
	fi
	printf '%s\n' "${srpm}"
}

# kernel.spec applies linux-kernel-test.patch with "git apply". git apply
# rejects several diffs of the same file in one patch, so the series cannot
# be joined as it is. Instead the series goes onto the vanilla source with
# "patch", and git writes one diff with one section for each file.
combined_series_patch() {
	local sources="$1" output="$2" tarball work
	tarball="$(find "${sources}" -maxdepth 1 -name 'linux-*.tar.xz' -print -quit)"
	[[ -n "${tarball}" ]] || fail "The source package has no linux-*.tar.xz."
	work="$(mktemp -d)"
	# The series changes only files below these folders.
	tar -xf "${tarball}" -C "${work}" --strip-components=1 \
		--wildcards '*/Makefile' '*/drivers/gpu' '*/drivers/video' '*/include'
	git -C "${work}" init --quiet
	git -C "${work}" add --all
	git -C "${work}" -c user.name=build -c user.email=build@localhost commit --quiet -m vanilla
	"${kernel_dir}/apply-series.sh" "${work}" >&2
	git -C "${work}" add --all
	git -C "${work}" diff --cached >"${output}"
	rm -rf "${work}"
}

add_series() {
	local spec="$1" sources="$2"
	info "Adding the patch series as linux-kernel-test.patch"
	# kernel.spec applies this file after the Fedora patches. It is empty in
	# the Fedora source package, so nothing of Fedora gets lost.
	combined_series_patch "${sources}" "${sources}/linux-kernel-test.patch"

	grep -q '^# define buildid \.local$' "${spec}" ||
		fail "kernel.spec has no line '# define buildid .local'. The spec format changed."
	sed -i "s/^# define buildid \\.local\$/%define buildid ${build_id}/" "${spec}"
}

# The host kernel of a container can lack hmac(sha512) in its crypto API.
# Then sha512hmac fails at the end of the build, and the stand-in in
# kernel/sha512hmac-fallback.py writes the same checksum file instead.
use_sha512hmac_fallback_if_needed() {
	if python3 -c 'import socket; socket.socket(socket.AF_ALG, socket.SOCK_SEQPACKET).bind(("hash", "hmac(sha512)"))' 2>/dev/null; then
		return 0
	fi
	local fallback_dir
	fallback_dir="$(mktemp -d)"
	install -m 0755 "${kernel_dir}/sha512hmac-fallback.py" "${fallback_dir}/sha512hmac"
	export PATH="${fallback_dir}:${PATH}"
	info "The kernel crypto API has no hmac(sha512). The build uses kernel/sha512hmac-fallback.py."
}

main() {
	[[ $# -ge 1 && $# -le 2 ]] || fail "Usage: kernel/build-fedora.sh KERNEL_VERSION_RELEASE [OUTPUT_DIR]"
	local nvr="$1" output_dir="${2:-${PWD}/kernel-rpms}"
	local fedora_release="${nvr##*.fc}"
	[[ "${fedora_release}" =~ ^[0-9]+$ ]] || fail "${nvr} has no Fedora release, for example 7.3.3-300.fc45."
	[[ "$(rpm -E %fedora)" == "${fedora_release}" ]] ||
		fail "This system is Fedora $(rpm -E %fedora). Build ${nvr} on Fedora ${fedora_release}."

	local topdir="${HTPC_BUILD_DIR:-${HOME}/.cache/x86-linux-htpc/rpmbuild}/kernel-${nvr}"
	mkdir -p "${topdir}"/{SRPMS,SOURCES,SPECS,BUILD,RPMS}

	local srpm
	srpm="$(download_source_package "${nvr}" "${topdir}")"
	rpm --define "_topdir ${topdir}" -i "${srpm}"
	local spec="${topdir}/SPECS/kernel.spec"

	add_series "${spec}" "${topdir}/SOURCES"

	if [[ "${EUID}" -eq 0 ]]; then
		info "Installing the build dependencies"
		# The dependency list of the full spec is longer than this build
		# needs. The extra packages do no harm in a build system.
		dnf builddep -y "${spec}"
	fi

	use_sha512hmac_fallback_if_needed
	info "Building. This takes from 30 minutes to some hours."
	rpmbuild --define "_topdir ${topdir}" -bb "${rpmbuild_options[@]}" "${spec}"

	mkdir -p "${output_dir}"
	find "${topdir}/RPMS" -name "*${build_id}*.rpm" -exec cp -t "${output_dir}" {} +
	info "Packages in ${output_dir}:"
	ls -1 "${output_dir}"
}

main "$@"
