#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Builds FFmpeg, libplacebo and the CB1 engine with the CB1 patches, at the
# versions that the pinned libreelec-x86-DV commit uses.
#
#   cb1/build.sh [PREFIX]
#   cb1/build.sh /opt/x86-linux-htpc/cb1
#
# All three install into PREFIX, and nothing replaces the FFmpeg or the
# libplacebo of the distribution. The libraries find each other through
# their run path, so no program needs LD_LIBRARY_PATH.
#
# Run it as a user that can write PREFIX, or in a container with
# cb1/build-in-container.sh.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cache_root="${XDG_CACHE_HOME:-${HOME}/.cache}/x86-linux-htpc"
jobs="$(nproc)"

fail() {
	printf 'ERROR: %s\n' "$*" >&2
	exit 1
}

# Progress messages go to stderr, because some functions print their result
# on stdout.
info() {
	printf '== %s\n' "$*" >&2
}

upstream_value() {
	sed -n "s/^$1=//p" "${repo_root}/kernel/upstream.conf"
}

# A git checkout of exactly one commit. git names each commit by the hash of
# its content, so the commit id is the checksum of the whole tree.
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

download_checked() {
	local url="$1" sha256="$2" file="$3"
	if [[ ! -f "${file}" ]]; then
		info "Downloading ${url}"
		curl -fsSL -o "${file}.part" "${url}" || fail "Download failed: ${url}"
		mv "${file}.part" "${file}"
	fi
	sha256sum --quiet --check <<<"${sha256}  ${file}" ||
		fail "Checksum mismatch for ${file}. Delete it and run the script again."
}

# Reads one value from CB1/config/dependencies.json, for example
# "ffmpeg.archive_sha256".
dependency_value() {
	python3 -I -c 'import json, sys
value = json.load(open(sys.argv[1]))
for key in sys.argv[2].split("."):
    value = value[key]
print(value)' "${upstream}/CB1/config/dependencies.json" "$1"
}

# Applies each patch of a folder in name order. LibreELEC applies them in
# the same order.
apply_patches() {
	local source_dir="$1" patch_dir="$2" patch_file
	for patch_file in "${patch_dir}"/*.patch; do
		patch -d "${source_dir}" -p1 --forward --no-backup-if-mismatch --quiet <"${patch_file}" ||
			fail "Patch does not apply: ${patch_file}"
		printf 'Applied %s\n' "$(basename "${patch_file}")" >&2
	done
}

build_ffmpeg() {
	local version url sha256 archive source
	version="$(dependency_value ffmpeg.version)"
	url="$(dependency_value ffmpeg.source_url)"
	sha256="$(dependency_value ffmpeg.archive_sha256)"
	archive="${cache_root}/cb1/ffmpeg-${version}.tar.xz"
	download_checked "${url}" "${sha256}" "${archive}"

	source="${work}/ffmpeg-${version}"
	tar -xf "${archive}" -C "${work}"
	# The FFmpeg patches of CB1 and of the Generic x86 project of LibreELEC.
	apply_patches "${source}" "${upstream}/projects/Generic/patches/ffmpeg"

	info "Building FFmpeg ${version}"
	# The configure options follow packages/multimedia/ffmpeg/package.mk of
	# LibreELEC. The encoders and the programs stay off, because players
	# need only the decoders, the demuxers and the filters. GnuTLS gives the
	# protocol https, for media addresses from DLNA apps and Home Assistant.
	(
		cd "${source}"
		./configure \
			--prefix="${prefix}" \
			--extra-ldflags="-Wl,-rpath,${prefix}/lib" \
			--disable-static --enable-shared \
			--enable-gpl --enable-version3 \
			--disable-doc --disable-programs \
			--disable-encoders --enable-encoder=ac3,eac3,aac,mjpeg,png \
			--disable-muxers --enable-muxer=spdif,adts,mpegts \
			--disable-devices \
			--enable-vaapi --enable-libdrm --enable-libdav1d --enable-gnutls \
			--disable-vdpau \
			--enable-runtime-cpudetect \
			--enable-pic
		make -j"${jobs}"
		make install
	) >&2
}

build_libplacebo() {
	local revision source
	revision="$(dependency_value libplacebo.upstream_revision)"
	source="${cache_root}/cb1/libplacebo-${revision}"
	checkout_commit https://github.com/haasn/libplacebo "${revision}" "${source}"
	# libplacebo takes glad, Jinja and fast_float from its git submodules.
	git -C "${source}" submodule update --quiet --init --depth 1

	rm -rf "${work}/libplacebo"
	cp -a "${source}" "${work}/libplacebo"
	apply_patches "${work}/libplacebo" \
		"${upstream}/packages/addons/addon-depends/multimedia-tools-depends/libplacebo/patches"

	info "Building libplacebo ${revision}"
	# The options follow the libplacebo package.mk of LibreELEC. CB1 needs
	# OpenGL and the Dolby Vision code, and it does not use Vulkan.
	meson setup "${work}/libplacebo-build" "${work}/libplacebo" \
		--prefix="${prefix}" --libdir=lib --buildtype=release \
		-Dc_link_args="-Wl,-rpath,${prefix}/lib" \
		-Dcpp_link_args="-Wl,-rpath,${prefix}/lib" \
		-Ddefault_library=shared \
		-Dopengl=enabled -Dgl-proc-addr=enabled \
		-Dvulkan=disabled -Dvk-proc-addr=disabled \
		-Dd3d11=disabled -Dglslang=disabled -Dshaderc=disabled \
		-Dlcms=disabled -Ddovi=enabled -Dlibdovi=disabled \
		-Ddemos=false -Dtests=false >&2
	ninja -C "${work}/libplacebo-build" install >&2
}

build_cb1() {
	local source="${upstream}/CB1"
	info "Checking the CB1 sources"
	(cd "${source}" && python3 -I tools/verify.py) >&2

	info "Building CB1"
	local cb1_version
	cb1_version="$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["CB1"]["version"])' \
		"${upstream}/config/cb1-versions.json")"
	cmake -S "${source}" -B "${work}/cb1-build" \
		-DCMAKE_BUILD_TYPE=Release -DCMAKE_PREFIX_PATH="${prefix}" >&2
	cmake --build "${work}/cb1-build" -j"${jobs}" >&2

	# CB1 has no install rule. Players link the static library and include
	# the headers of src/, as the Kodi build of LibreELEC does.
	install -D -m 0644 "${work}/cb1-build/src/libdvbridge_core.a" "${prefix}/lib/libdvbridge_core.a"
	install -d "${prefix}/include/cb1"
	install -m 0644 "${source}"/src/*.h "${work}"/cb1-build/src/*.h "${prefix}/include/cb1/"
	install -D -m 0644 /dev/stdin "${prefix}/lib/pkgconfig/cb1.pc" <<EOF
prefix=${prefix}
libdir=\${prefix}/lib
includedir=\${prefix}/include/cb1

Name: cb1
Description: CB1 Dolby Vision engine
Version: ${cb1_version}
Requires: libplacebo libavutil libavcodec glesv2 egl libdrm libva
Libs: -L\${libdir} -ldvbridge_core -ldl -lpthread -lm
Cflags: -I\${includedir}
EOF
	install -d "${prefix}/share/licenses/cb1"
	cp -r "${source}/LICENSE" "${source}/LICENSES" "${prefix}/share/licenses/cb1/"
}

main() {
	[[ $# -le 1 ]] || fail "Usage: cb1/build.sh [PREFIX]"
	prefix="${1:-/opt/x86-linux-htpc/cb1}"
	mkdir -p "${prefix}" "${cache_root}/cb1"
	[[ -w "${prefix}" ]] || fail "${prefix} is not writable."

	local commit
	commit="$(upstream_value UPSTREAM_COMMIT)"
	upstream="${cache_root}/cb1/libreelec-x86-DV-${commit}"
	checkout_commit "$(upstream_value UPSTREAM_REPO)" "${commit}" "${upstream}"

	work="$(mktemp -d)"
	trap 'rm -rf "${work}"' EXIT
	export PKG_CONFIG_PATH="${prefix}/lib/pkgconfig${PKG_CONFIG_PATH:+:${PKG_CONFIG_PATH}}"

	build_ffmpeg
	build_libplacebo
	build_cb1
	printf '%s\n' "${commit}" >"${prefix}/libreelec-x86-DV-commit"
	info "Done. The libraries are in ${prefix}."
}

main "$@"
