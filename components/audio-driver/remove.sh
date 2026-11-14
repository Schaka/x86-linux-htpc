#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

remove_file /etc/modprobe.d/x86-linux-htpc-audio.conf
info "Restart the machine to load the default audio driver."
