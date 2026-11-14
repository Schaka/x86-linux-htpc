#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Puts the codec packages of the distribution back. RPM Fusion stays
# enabled, because other packages can depend on it.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

case "$(distribution_family)" in
fedora)
	fedora_swap_package ffmpeg ffmpeg-free
	fedora_swap_package intel-media-driver libva-intel-media-driver
	;;
esac
