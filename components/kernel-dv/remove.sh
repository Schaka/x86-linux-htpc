#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Makes the newest stock kernel the default and removes the patched kernel
# packages. dnf does not remove the running kernel, so start a stock kernel
# first.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

[[ "$(uname -r)" != *.htpc.* ]] || fail "The patched kernel runs. Start a stock kernel first."
stock_kernel="$(find /boot -maxdepth 1 -name 'vmlinuz-*' ! -name '*.htpc.*' ! -name '*rescue*' | sort -V | tail -n 1)"
[[ -n "${stock_kernel}" ]] || fail "No stock kernel in /boot. Install one first."
remove_file /etc/kernel/install.d/96-x86-linux-htpc-default.install
grubby --set-default "${stock_kernel}"
mapfile -t patched < <(rpm -qa 'kernel*' | grep '\.htpc\.')
if [[ "${#patched[@]}" -gt 0 ]]; then
	dnf remove -y "${patched[@]}"
fi
