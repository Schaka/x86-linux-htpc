#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Builds htpc-control as one static program, for the component control-api.
#
#   control/build.sh [OUTPUT_DIR]
#
# The default OUTPUT_DIR is /var/tmp/htpc-control, the default of
# CONTROL_BUILD_DIR. With cargo on the machine, the build runs there and
# needs the target x86_64-unknown-linux-musl ("rustup target add
# x86_64-unknown-linux-musl"). Without cargo, it runs in an Alpine
# container with podman or docker. The program then works on each x86-64
# Linux distribution, because it links no library of the system.
set -euo pipefail

source_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
output_dir="${1:-/var/tmp/htpc-control}"
target=x86_64-unknown-linux-musl
# The Alpine image of Rust builds for musl by default.
container_image=docker.io/library/rust:1-alpine

fail() {
	printf 'ERROR: %s\n' "$*" >&2
	exit 1
}

build_with_cargo() {
	rustup target list --installed 2>/dev/null | grep -qx "${target}" ||
		fail "Add the musl target first: rustup target add ${target}"
	# ring compiles some C code. Without a musl-gcc, the gcc of the system
	# works for it, because that code uses no headers of the C library.
	command -v musl-gcc >/dev/null 2>&1 || export CC_x86_64_unknown_linux_musl="${CC:-gcc}"
	cargo build --release --locked --target "${target}" --manifest-path "${source_dir}/Cargo.toml"
	install -D -m 0755 "${source_dir}/target/${target}/release/htpc-control" "${output_dir}/htpc-control"
}

build_in_container() {
	local engine
	engine="$(command -v podman || command -v docker || true)"
	[[ -n "${engine}" ]] || fail "Install cargo, podman or docker."
	mkdir -p "${output_dir}"
	"${engine}" run --rm \
		-v "${source_dir}:/src:ro,z" -v "${output_dir}:/out:z" \
		"${container_image}" sh -euc '
			apk add --no-cache musl-dev >/dev/null
			cp -r /src /build
			cd /build
			rm -rf target
			cargo build --release --locked
			install -m 0755 target/release/htpc-control /out/htpc-control'
}

if command -v cargo >/dev/null 2>&1; then
	build_with_cargo
else
	build_in_container
fi
"${output_dir}/htpc-control" openapi >"${output_dir}/openapi.yaml"
printf 'Built %s\n' "${output_dir}/htpc-control"
