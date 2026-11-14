#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Builds mpv against the FFmpeg and the libplacebo of cb1/build.sh, with the
# patches in mpv/patches/.
#
#   mpv/build.sh [PREFIX]
#   mpv/build.sh /opt/x86-linux-htpc/cb1
#
# Run cb1/build.sh with the same PREFIX first. mpv installs into PREFIX
# next to the libraries, and its run path points there, so this mpv never
# loads the FFmpeg or the libplacebo of the distribution.
set -euo pipefail

mpv_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cache_root="${XDG_CACHE_HOME:-${HOME}/.cache}/x86-linux-htpc"

# mpv 0.41.0, the version of Fedora 45. git names each commit by the hash
# of its content, so the commit id also checks the download.
mpv_url=https://github.com/mpv-player/mpv
mpv_commit=41f6a645068483470267271e1d09966ca3b9f413

fail() {
	printf 'ERROR: %s\n' "$*" >&2
	exit 1
}

info() {
	printf '== %s\n' "$*" >&2
}

checkout_commit() {
	local url="$1" commit="$2" dir="$3"
	if [[ "$(git -C "${dir}" rev-parse HEAD 2>/dev/null)" != "${commit}" ]]; then
		info "Downloading ${url} at ${commit}"
		rm -rf "${dir}"
		git init --quiet "${dir}"
		git -C "${dir}" fetch --quiet --depth 1 "${url}" "${commit}"
		git -C "${dir}" -c advice.detachedHead=false checkout --quiet FETCH_HEAD
	fi
}

main() {
	[[ $# -le 1 ]] || fail "Usage: mpv/build.sh [PREFIX]"
	local prefix="${1:-/opt/x86-linux-htpc/cb1}"
	[[ -f "${prefix}/lib/pkgconfig/libplacebo.pc" ]] ||
		fail "${prefix} has no libplacebo. Run cb1/build.sh ${prefix} first."

	local source="${cache_root}/mpv/mpv-${mpv_commit}"
	mkdir -p "${cache_root}/mpv"
	checkout_commit "${mpv_url}" "${mpv_commit}" "${source}"

	local patch_file
	# Global, because the EXIT trap runs after main returns.
	work="$(mktemp -d)"
	trap 'rm -rf "${work}"' EXIT
	cp -a "${source}" "${work}/mpv"
	for patch_file in "${mpv_dir}"/patches/*.patch; do
		[[ -e "${patch_file}" ]] || continue
		patch -d "${work}/mpv" -p1 --forward --no-backup-if-mismatch --quiet <"${patch_file}" ||
			fail "Patch does not apply: ${patch_file}"
		info "Applied $(basename "${patch_file}")"
	done

	info "Building mpv"
	export PKG_CONFIG_PATH="${prefix}/lib/pkgconfig${PKG_CONFIG_PATH:+:${PKG_CONFIG_PATH}}"
	# libmpv is for jellyfin-mpv-shim. The DRM output with EGL and GBM runs
	# mpv on its own VT for Dolby Vision. Wayland stays on, so the same mpv
	# also runs inside Plasma for tests. Vulkan is off, because CB1 needs
	# OpenGL and its libplacebo has no Vulkan.
	meson setup "${work}/build" "${work}/mpv" \
		--prefix="${prefix}" --libdir=lib --buildtype=release \
		-Dc_link_args="-Wl,-rpath,${prefix}/lib" \
		-Dgpl=true -Dcplayer=true -Dlibmpv=true \
		-Ddrm=enabled -Dgbm=enabled -Degl=enabled -Degl-drm=enabled -Dgl=enabled \
		-Dvaapi=enabled -Dvaapi-drm=enabled \
		-Dwayland=enabled -Degl-wayland=enabled -Dvaapi-wayland=enabled -Ddmabuf-wayland=enabled \
		-Dvulkan=disabled -Dx11=disabled \
		-Dpipewire=enabled -Dalsa=enabled -Dpulse=disabled \
		-Dcb1=enabled \
		-Dlua=enabled -Djavascript=disabled \
		-Dlibarchive=disabled -Dlibbluray=disabled -Ddvdnav=disabled -Dcdda=disabled \
		-Dmanpage-build=disabled -Dhtml-build=disabled -Dpdf-build=disabled >&2
	ninja -C "${work}/build" install >&2
	info "Done. mpv is ${prefix}/bin/mpv."
}

main "$@"
