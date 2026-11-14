#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Runs cb1/build.sh, mpv/build.sh and kodi/build.sh in a Fedora container
# and packs the result.
#
#   cb1/build-in-container.sh [--no-kodi] FEDORA_RELEASE OUTPUT_DIR
#   cb1/build-in-container.sh 45 /var/tmp/htpc-cb1
#
# The result is OUTPUT_DIR/x86-linux-htpc-cb1-fcN.tar.gz with the folder
# /opt/x86-linux-htpc/cb1, and OUTPUT_DIR/x86-linux-htpc-kodi-fcN.tar.gz with
# /opt/x86-linux-htpc/kodi. Build for the Fedora release of the HTPC,
# because the libraries link the system libraries of that release. The
# components cb1 and kodi-dv install the archives.
#
# The build needs podman, about 15 GB of disk space and, on a computer with
# 4 cores, about 2 hours. A faster computer can build for the HTPC, because
# the build runs in a container of the same Fedora release.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
prefix=/opt/x86-linux-htpc/cb1
kodi_prefix=/opt/x86-linux-htpc/kodi

fail() {
	printf 'ERROR: %s\n' "$*" >&2
	exit 1
}

build_packages=(
	gcc gcc-c++ make cmake meson ninja-build nasm pkgconf-pkg-config
	patch curl git-core xz python3 python3-pyyaml
	libva-devel libdrm-devel libdav1d-devel
	mesa-libEGL-devel mesa-libGLES-devel mesa-libgbm-devel
	zlib-devel bzip2-devel
	libass-devel pipewire-devel alsa-lib-devel luajit-devel
	wayland-devel wayland-protocols-devel libxkbcommon-devel libdisplay-info-devel
	lcms2-devel zimg-devel uchardet-devel libjpeg-turbo-devel systemd-devel
)

# Kodi takes most of its build packages from the build dependencies of the
# Kodi package of RPM Fusion. These come on top: Java for the code
# generator of Kodi, the parts for GBM, CEC and PipeWire, and the libraries
# of the own Python of Kodi.
kodi_packages=(
	java-latest-openjdk-headless libinput-devel libcec-devel libudfread-devel
	dnf-plugins-core
	openssl-devel libffi-devel xz-devel sqlite-devel libuuid-devel bzip2-devel zlib-devel
	ncurses-devel gdbm-devel cpio openssl3-libs
)

main() {
	local with_kodi=yes
	if [[ "${1:-}" == "--no-kodi" ]]; then
		with_kodi=no
		shift
	fi
	[[ $# -eq 2 ]] || fail "Usage: cb1/build-in-container.sh [--no-kodi] FEDORA_RELEASE OUTPUT_DIR"
	local fedora_release="$1" output_dir="$2"
	[[ "${fedora_release}" =~ ^[0-9]+$ ]] || fail "FEDORA_RELEASE is a number, for example 45."
	command -v podman >/dev/null || fail "This script needs podman."

	local cache="${XDG_CACHE_HOME:-${HOME}/.cache}/x86-linux-htpc"
	mkdir -p "${cache}/container-home" "${output_dir}"
	output_dir="$(cd "${output_dir}" && pwd)"

	podman run --rm \
		--volume "${repo_root}:/repo:ro,z" \
		--volume "${cache}/container-home:/root:z" \
		--volume "${output_dir}:/output:z" \
		--env "WITH_KODI=${with_kodi}" --env "FEDORA_RELEASE=${fedora_release}" \
		"registry.fedoraproject.org/fedora:${fedora_release}" \
		bash -c "set -e
			dnf install -y ${build_packages[*]} >/dev/null
			/repo/cb1/build.sh ${prefix}
			/repo/mpv/build.sh ${prefix}
			tar -czf /output/x86-linux-htpc-cb1-fc\${FEDORA_RELEASE}.tar.gz -C / ${prefix#/}
			if [ \"\${WITH_KODI}\" = yes ]; then
				dnf install -y https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-\${FEDORA_RELEASE}.noarch.rpm >/dev/null
				dnf install -y --skip-unavailable ${kodi_packages[*]} >/dev/null
				dnf builddep -y kodi >/dev/null
				# Python 3.14 does not build with OpenSSL 4. Fedora also has
				# OpenSSL 3. Its libraries install next to OpenSSL 4, but its
				# devel package conflicts, so its headers go into a folder of
				# their own.
				mkdir -p /tmp/openssl3/rpms && cd /tmp/openssl3/rpms
				dnf download -q openssl3-devel
				cd /tmp/openssl3 && for rpm in rpms/*.rpm; do rpm2cpio \"\$rpm\" | cpio -idm --quiet 2>/dev/null; done
				ln -sfn usr/include include && ln -sfn usr/lib64 lib
				# The links of the devel package point to the libraries of
				# openssl3-libs, which are in /usr/lib64.
				ln -sf /usr/lib64/libssl.so.3 usr/lib64/libssl.so
				ln -sf /usr/lib64/libcrypto.so.3 usr/lib64/libcrypto.so
				mkdir -p ${kodi_prefix}
				KODI_OPENSSL_DIR=/tmp/openssl3 /repo/kodi/build.sh ${prefix} ${kodi_prefix}
				tar -czf /output/x86-linux-htpc-kodi-fc\${FEDORA_RELEASE}.tar.gz -C / ${kodi_prefix#/}
			fi"
}

main "$@"
