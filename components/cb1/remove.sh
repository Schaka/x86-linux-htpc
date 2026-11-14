#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

rm -rf /opt/x86-linux-htpc/cb1
info "Removed /opt/x86-linux-htpc/cb1"
