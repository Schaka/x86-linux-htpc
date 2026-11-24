#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Builds Kodi with the CB1 engine, at the Kodi version and with the patches
# of the pinned libreelec-x86-DV commit.
#
#   kodi/build.sh [CB1_PREFIX] [PREFIX]
#   kodi/build.sh /opt/x86-linux-htpc/cb1 /opt/x86-linux-htpc/kodi
#
# Run cb1/build.sh with CB1_PREFIX first. Kodi links the FFmpeg and the
# libplacebo of that build, and its run path points there. The Kodi build
# compiles the CB1 engine from the CB1 sources of libreelec-x86-DV itself.
#
# Kodi runs on its own VT with the GBM platform and OpenGL ES, as the
# Generic image of libreelec-x86-DV does. The patches of LibreELEC for its
# own system (updates, mounts, service wrappers) stay out, because Fedora
# does these jobs itself.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cache_root="${XDG_CACHE_HOME:-${HOME}/.cache}/x86-linux-htpc"
jobs="$(nproc)"

fail() {
	printf 'ERROR: %s\n' "$*" >&2
	exit 1
}

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

# Reads a variable such as PKG_VERSION from the Kodi package of LibreELEC.
package_value() {
	sed -n "s/^$1=\"\(.*\)\"$/\1/p" "${upstream}/packages/mediacenter/kodi/package.mk"
}

# Kodi needs a newer libdisplay-info than some distributions have, for
# example Fedora 45 with 0.3. It goes into PREFIX, so the system keeps its
# own version.
build_libdisplay_info() {
	local version sha256 archive
	version="$(sed -n 's/^PKG_VERSION="\(.*\)"$/\1/p' "${upstream}/packages/devel/libdisplay-info/package.mk")"
	sha256="$(sed -n 's/^PKG_SHA256="\(.*\)"$/\1/p' "${upstream}/packages/devel/libdisplay-info/package.mk")"
	archive="${cache_root}/kodi/libdisplay-info-${version}.tar.xz"
	download_checked "https://gitlab.freedesktop.org/emersion/libdisplay-info/-/releases/${version}/downloads/libdisplay-info-${version}.tar.xz" \
		"${sha256}" "${archive}"
	tar -xJf "${archive}" -C "${work}"
	info "Building libdisplay-info ${version}"
	meson setup "${work}/libdisplay-info-build" "${work}/libdisplay-info-${version}" \
		--prefix="${prefix}" --libdir=lib --buildtype=release >&2
	ninja -C "${work}/libdisplay-info-build" install >&2
}

# The HDR10 AI of CB1 estimates the Dolby Vision brightness metadata of an
# HDR10 video with a LightGBM model. The CB1 patches of Kodi use its fields
# also when the option is off, so the build needs it. LibreELEC builds the
# C library from the Python source package.
build_lightgbm() {
	local mk="${upstream}/packages/multimedia/lightgbm/package.mk" version sha256 url archive
	version="$(sed -n 's/^PKG_VERSION="\(.*\)"$/\1/p' "${mk}")"
	sha256="$(sed -n 's/^PKG_SHA256="\(.*\)"$/\1/p' "${mk}")"
	url="$(sed -n 's/^PKG_URL="\(.*\)"$/\1/p' "${mk}")"
	url="${url//\$\{PKG_VERSION\}/${version}}"
	archive="${cache_root}/kodi/lightgbm-${version}.tar.gz"
	download_checked "${url}" "${sha256}" "${archive}"
	tar -xzf "${archive}" -C "${work}"
	info "Building LightGBM ${version}"
	cmake -S "${work}/lightgbm-${version}" -B "${work}/lightgbm-build" -G Ninja \
		-DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="${prefix}" -DCMAKE_INSTALL_LIBDIR=lib \
		-DBUILD_CLI=OFF -DUSE_OPENMP=OFF -DUSE_GPU=OFF -DUSE_CUDA=OFF -DUSE_MPI=OFF \
		-DINSTALL_HEADERS=ON >&2
	cmake --build "${work}/lightgbm-build" -j"${jobs}" >&2
	cmake --install "${work}/lightgbm-build" >&2
}

# Kodi runs its add-ons in the Python that it links. Kodi add-ons follow the
# Python of the Kodi platforms, not the newer Python of some distributions.
# For example, typing_extensions of the Kodi add-on repository fails with
# Python 3.15, and the Jellyfin add-on then does not start. So Kodi gets
# the Python version of libreelec-x86-DV in PREFIX. It does not replace the
# Python of the system, and the system Python stays for all other programs.
#
# An older Python can need an older OpenSSL than the distribution, for
# example Python 3.14 with the OpenSSL 4 of Fedora 45. Then KODI_OPENSSL_DIR
# names a folder with include/ and lib/ of an OpenSSL 3, and the Python
# links it. cb1/build-in-container.sh prepares this folder on Fedora.
build_python() {
	local mk="${upstream}/packages/lang/Python3/package.mk" version sha256 archive
	version="$(sed -n 's/^PKG_VERSION="\(.*\)"$/\1/p' "${mk}")"
	sha256="$(sed -n 's/^PKG_SHA256="\(.*\)"$/\1/p' "${mk}")"
	archive="${cache_root}/kodi/Python-${version}.tar.xz"
	download_checked "https://www.python.org/ftp/python/${version}/Python-${version}.tar.xz" "${sha256}" "${archive}"
	tar -xJf "${archive}" -C "${work}"
	info "Building Python ${version}"
	(
		cd "${work}/Python-${version}"
		local -a ssl=()
		if [[ -n "${KODI_OPENSSL_DIR:-}" ]]; then
			ssl=(--with-openssl="${KODI_OPENSSL_DIR}" --with-openssl-rpath=no)
		fi
		./configure --prefix="${prefix}" --enable-shared --with-ensurepip=no \
			--disable-test-modules --without-static-libpython "${ssl[@]}" \
			LDFLAGS="-Wl,-rpath,${prefix}/lib"
		make -j"${jobs}"
		make install
	) >&2
	python_version="${version%.*}"
	# A Python module that did not build fails only at run time in an
	# add-on, so the build stops here instead.
	"${prefix}/bin/python${python_version}" -c "import ssl, sqlite3, ctypes, lzma, bz2, zlib, json" ||
		fail "The Python of Kodi misses a module."
}

kodi_patches() {
	# The patches of the Kodi package that also fit a normal distribution:
	# key maps for remotes, the menu language and the headers-only build.
	# The 1000 series is for the LibreELEC system.
	printf '%s\n' "${upstream}"/packages/mediacenter/kodi/patches/9*.patch
	# The CB1 patches of the Generic x86 project. The two patches of the
	# blur font style build on the patch 1002 of the 1000 series. Only a
	# skin can use this style, and Estuary does not, so they stay out.
	printf '%s\n' "${upstream}"/projects/Generic/patches/kodi/*.patch | grep -v -- '-blur-'
}

main() {
	[[ $# -le 2 ]] || fail "Usage: kodi/build.sh [CB1_PREFIX] [PREFIX]"
	local cb1_prefix="${1:-/opt/x86-linux-htpc/cb1}" prefix="${2:-/opt/x86-linux-htpc/kodi}"
	[[ -f "${cb1_prefix}/lib/pkgconfig/libplacebo.pc" ]] ||
		fail "${cb1_prefix} has no libplacebo. Run cb1/build.sh ${cb1_prefix} first."
	mkdir -p "${prefix}" "${cache_root}/kodi"
	[[ -w "${prefix}" ]] || fail "${prefix} is not writable."

	local commit
	commit="$(upstream_value UPSTREAM_COMMIT)"
	upstream="${cache_root}/cb1/libreelec-x86-DV-${commit}"
	checkout_commit "$(upstream_value UPSTREAM_REPO)" "${commit}" "${upstream}"

	local version sha256 archive
	version="$(package_value PKG_VERSION)"
	sha256="$(package_value PKG_SHA256)"
	archive="${cache_root}/kodi/kodi-${version}.tar.gz"
	download_checked "https://github.com/xbmc/xbmc/archive/${version}.tar.gz" "${sha256}" "${archive}"

	# Global, because the EXIT trap runs after main returns.
	work="$(mktemp -d)"
	trap 'rm -rf "${work}"' EXIT
	tar -xzf "${archive}" -C "${work}"
	local source="${work}/xbmc-${version}" patch_file
	while read -r patch_file; do
		patch -d "${source}" -p1 --forward --no-backup-if-mismatch --quiet <"${patch_file}" ||
			fail "Patch does not apply: ${patch_file}"
		info "Applied $(basename "${patch_file}")"
	done < <(kodi_patches)

	export PKG_CONFIG_PATH="${prefix}/lib/pkgconfig:${cb1_prefix}/lib/pkgconfig${PKG_CONFIG_PATH:+:${PKG_CONFIG_PATH}}"
	build_libdisplay_info
	build_lightgbm
	build_python
	local model="${upstream}/packages/mediacenter/kodi/cb1-model"
	local model_dir="${prefix}/share/cb1/l1l3"

	info "Building Kodi ${version}"
	# The options follow packages/mediacenter/kodi/package.mk of
	# libreelec-x86-DV for the device Generic. Kodi plays through PipeWire,
	# as the other players of the HTPC do, and ALSA stays as a second way.
	# Optical discs, network shares and AirPlay are off, because the HTPC
	# plays from Jellyfin and local files.
	cmake -S "${source}" -B "${work}/build" -G Ninja \
		-DCMAKE_BUILD_TYPE=Release \
		-DCMAKE_INSTALL_PREFIX="${prefix}" \
		-DCMAKE_PREFIX_PATH="${prefix};${cb1_prefix}" \
		-DCMAKE_INSTALL_RPATH="${prefix}/lib;${cb1_prefix}/lib" \
		-DCMAKE_BUILD_WITH_INSTALL_RPATH=ON \
		-DCORE_PLATFORM_NAME=gbm -DAPP_RENDER_SYSTEM=gles \
		-DCMAKE_C_FLAGS="-DEGL_NO_X11 -DCB1_L1L3_BUNDLE_DIR=\\\"${model_dir}\\\"" \
		-DCMAKE_CXX_FLAGS="-DEGL_NO_X11 -DCB1_L1L3_BUNDLE_DIR=\\\"${model_dir}\\\"" \
		"-DCORE_COMPILE_OPTIONS=-Werror=missing-field-initializers;-Werror=sign-compare" \
		-DENABLE_DVBRIDGE=ON -DCB1_ENABLE_HDR10_AI=ON \
		-DCB1_L1L3_BUNDLE_CONTRACT="${model}/contract" \
		-DCB1_LIGHTGBM_INCLUDE_DIR="${prefix}/include" \
		-DCB1_LIGHTGBM_LIBRARY="${prefix}/lib/lib_lightgbm.so" \
		-DDVBRIDGE_SOURCE_DIR="${upstream}/CB1/src" \
		-DFFMPEG_PATH="${cb1_prefix}" -DENABLE_INTERNAL_FFMPEG=OFF \
		-DPYTHON_PATH="${prefix}" -DPYTHON_VER="${python_version}" \
		-DENABLE_VAAPI=ON -DENABLE_CEC=ON \
		-DENABLE_PIPEWIRE=ON -DENABLE_ALSA=ON -DENABLE_PULSEAUDIO=OFF \
		-DENABLE_UDEV=ON -DENABLE_DBUS=ON -DENABLE_XSLT=ON \
		-DENABLE_EVENTCLIENTS=OFF -DENABLE_LIRCCLIENT=OFF -DENABLE_BLUETOOTH=OFF \
		-DENABLE_LCMS2=OFF -DENABLE_TESTING=OFF -DENABLE_CCACHE=OFF \
		-DENABLE_OPTICAL=OFF -DENABLE_DVDCSS=OFF -DENABLE_BLURAY=OFF \
		-DENABLE_SMBCLIENT=OFF -DENABLE_NFS=OFF -DENABLE_UPNP=OFF \
		-DENABLE_AIRTUNES=OFF -DENABLE_PLIST=OFF -DENABLE_AVAHI=OFF \
		-DENABLE_MARIADBCLIENT=OFF -DENABLE_MYSQLCLIENT=OFF \
		-DADDONS_CONFIGURE_AT_STARTUP=OFF \
		-DGIT_VERSION="${version}" >&2
	cmake --build "${work}/build" -j"${jobs}" >&2
	cmake --install "${work}/build" >&2
	install -d "${model_dir}"
	cp "${model}"/data/* "${model}/LICENSE.md" "${model_dir}/"
	printf '%s\n' "${commit}" >"${prefix}/libreelec-x86-DV-commit"
	info "Done. Kodi is ${prefix}/bin/kodi."
}

main "$@"
