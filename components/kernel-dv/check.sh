#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"

release="$(uname -r)"
drm_state="$(drm_info 2>/dev/null || true)"

running_patched_kernel() {
	[[ "${release}" == *.htpc.* ]]
}

connector_has() {
	grep -q "\"$1\"" <<<"${drm_state}"
}

dkms_modules_built() {
	command -v dkms >/dev/null || return 0
	local status
	status="$(dkms status -k "${release}")"
	! grep -v -q ': installed' <<<"${status}"
}

patched_kernel_is_default() {
	[[ "$(grubby --default-kernel 2>/dev/null)" == /boot/vmlinuz-*.htpc.* ]]
}

check "The running kernel is the patched kernel" running_patched_kernel
check "An HDMI connector has the Dolby Vision property" connector_has DVBRIDGE_DV_STANDARD_LAB
check "An HDMI connector has the HDR10+ property" connector_has HDR10PLUS_METADATA
check "DKMS built all its modules for the running kernel" dkms_modules_built
check "Stock kernel updates do not replace the patched default kernel" test -x /etc/kernel/install.d/96-x86-linux-htpc-default.install
if patched_kernel_is_default; then
	check_pass "The patched kernel is the default kernel"
else
	warn "The default kernel is not the patched kernel. After a good test start, run ./install.sh kernel-dv again."
fi

checks_done
