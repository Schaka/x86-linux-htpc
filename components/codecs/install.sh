#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Installs the full FFmpeg and the full Intel VA-API driver. The players
# need them for HEVC, AC-3, E-AC-3, TrueHD and DTS, and for the IEC 61937
# packer of audio passthrough.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

case "$(distribution_family)" in
fedora)
	fedora_enable_rpmfusion
	fedora_swap_package ffmpeg-free ffmpeg
	fedora_swap_package libva-intel-media-driver intel-media-driver
	;;
esac

install_packages "$(dirname "${BASH_SOURCE[0]}")"
