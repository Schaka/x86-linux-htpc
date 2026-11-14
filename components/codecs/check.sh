#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

render_node="$(find /dev/dri -name 'renderD*' -print -quit 2>/dev/null || true)"

# The lists are read once into variables. A pipe into "grep -q" would end
# the writer early, and pipefail would report that as a failure.
ffmpeg_decoders="$(ffmpeg -hide_banner -decoders 2>/dev/null || true)"
ffmpeg_muxers="$(ffmpeg -hide_banner -muxers 2>/dev/null || true)"
vaapi_profiles=""
if [[ -n "${render_node}" ]]; then
	vaapi_profiles="$(vainfo --display drm --device "${render_node}" 2>/dev/null || true)"
fi

ffmpeg_has_decoder() {
	grep -qw -- "$1" <<<"${ffmpeg_decoders}"
}

ffmpeg_has_muxer() {
	grep -qw -- "$1" <<<"${ffmpeg_muxers}"
}

vaapi_decodes() {
	grep -E -- "$1" <<<"${vaapi_profiles}" | grep -q VAEntrypointVLD
}

case "$(distribution_family)" in
fedora) check "FFmpeg is the full RPM Fusion build" package_installed ffmpeg ;;
esac

for decoder in hevc ac3 eac3 truehd dca; do
	check "FFmpeg decodes ${decoder}" ffmpeg_has_decoder "${decoder}"
done
check "FFmpeg has the IEC 61937 packer (spdif muxer)" ffmpeg_has_muxer spdif

if [[ -z "${render_node}" ]]; then
	check_fail "A GPU render node exists in /dev/dri"
else
	check "VA-API decodes HEVC Main 10" vaapi_decodes "VAProfileHEVCMain10"
	check "VA-API decodes H.264" vaapi_decodes "VAProfileH264High"
	if vaapi_decodes "VAProfileAV1Profile0"; then
		check_pass "VA-API decodes AV1"
	else
		warn "VA-API does not decode AV1. Older GPUs have no AV1 decoder."
	fi
fi

checks_done
