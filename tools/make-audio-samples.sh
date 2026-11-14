#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Creates audio channel checks in each codec that a receiver can decode.
#
#   tools/make-audio-samples.sh [OUTPUT_DIR]
#
# Each file plays a 440 Hz tone on one channel after the other, 2 seconds
# each, over a black 1080p picture at 23.976 frames per second. TrueHD
# needs the HBR mode of HDMI like Dolby Atmos and DTS-HD Master Audio, so
# it tests the same transport. FFmpeg cannot write Atmos, DTS-HD or DTS:X.
set -euo pipefail

output_dir="${1:-${HOME}/Videos/htpc-samples}"
mkdir -p "${output_dir}"

# One aevalsrc expression for each channel. Channel n sounds during the
# seconds [2n, 2n+2) of each cycle of 2 * channels seconds.
tone_expression() {
	local channels="$1" i expression=""
	for ((i = 0; i < channels; i++)); do
		[[ "${i}" -eq 0 ]] || expression+="|"
		expression+="0.5*sin(440*2*PI*t)*between(mod(t\\,$((2 * channels)))\\,$((2 * i))\\,$((2 * i + 2))-0.001)"
	done
	printf '%s\n' "${expression}"
}

make_sample() {
	local name="$1" layout="$2" channels="$3"
	shift 3
	local target="${output_dir}/${name}" duration=$((4 * channels))
	if [[ -s "${target}" ]]; then
		printf 'Exists: %s\n' "${target}"
		return 0
	fi
	ffmpeg -hide_banner -loglevel error -y \
		-f lavfi -i "color=c=black:s=1920x1080:r=24000/1001" \
		-f lavfi -i "aevalsrc=$(tone_expression "${channels}"):s=48000:c=${layout}:d=${duration}" \
		-map 0:v -map 1:a -c:v libx264 -preset ultrafast -tune stillimage -t "${duration}" \
		"$@" "${target}"
	printf 'Created: %s\n' "${target}"
}

make_sample audio-ac3-5.1.mkv 5.1 6 -c:a ac3 -b:a 640k
make_sample audio-eac3-5.1.mkv 5.1 6 -c:a eac3 -b:a 1024k
make_sample audio-dts-5.1.mkv 5.1 6 -c:a dca -b:a 1509k -strict -2
make_sample audio-truehd-5.1.mkv 5.1 6 -c:a truehd -strict -2
make_sample audio-pcm-7.1.mkv 7.1 8 -c:a pcm_s24le
