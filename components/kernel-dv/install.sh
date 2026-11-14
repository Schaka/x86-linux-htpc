#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Installs the patched kernel next to the stock kernel, in two runs:
#
#   1. The first run installs the packages from KERNEL_PACKAGES_DIR, keeps the
#      old default kernel and boots the patched kernel once at the next start.
#   2. After that start, the second run makes the patched kernel the default,
#      but only if its checks pass.
#
# So a kernel that does not start costs one power cycle and nothing else.
set -euo pipefail
# shellcheck source=../../lib/htpc.sh
source "$(dirname "${BASH_SOURCE[0]}")/../../lib/htpc.sh"
require_root

component_dir="$(dirname "${BASH_SOURCE[0]}")"
release_tag=".htpc."

# The packages that a working kernel needs. kernel-devel lets DKMS build its
# modules before the first start of the kernel, for example a fix that the
# network of the machine depends on.
kernel_packages=(kernel kernel-core kernel-modules kernel-modules-core kernel-modules-extra kernel-devel)

package_files() {
	local dir="$1" name file
	for name in "${kernel_packages[@]}"; do
		file="$(find "${dir}" -maxdepth 1 -name "${name}-[0-9]*${release_tag}*.rpm" -print -quit)"
		[[ -n "${file}" ]] || fail "${dir} has no ${name} package with ${release_tag} in its name. See docs/kernel.md."
		printf '%s\n' "${file}"
	done
}

# Prints the kernel release, for example 7.3.3-300.htpc.fc45.x86_64, from
# the name of the kernel-core file in the list of package files.
kernel_release_of() {
	local file core_name
	for file in "$@"; do
		core_name="$(basename "${file}")"
		if [[ "${core_name}" == kernel-core-* ]]; then
			core_name="${core_name#kernel-core-}"
			printf '%s\n' "${core_name%.rpm}"
			return 0
		fi
	done
	fail "The package list has no kernel-core package."
}

boot_entry_of() {
	local release="$1" entry
	entry="$(find /boot/loader/entries -name "*-${release}.conf" -print -quit)"
	[[ -n "${entry}" ]] || fail "No boot entry for ${release} in /boot/loader/entries."
	basename "${entry}" .conf
}

dkms_ready_for() {
	local release="$1" status
	command -v dkms >/dev/null || return 0
	status="$(dkms status -k "${release}")"
	if grep -v -q ': installed' <<<"${status}"; then
		printf '%s\n' "${status}" >&2
		return 1
	fi
}

install_and_boot_once() {
	local -a files=("$@")
	local release old_default
	release="$(kernel_release_of "${files[@]}")"
	old_default="$(grubby --default-kernel)"

	dnf install -y "${files[@]}"
	# kernel-install makes each new kernel the default. The patched kernel
	# must first prove itself with one start.
	grubby --set-default "${old_default}"

	dkms_ready_for "${release}" ||
		fail "DKMS did not build all modules for ${release}. Do not start this kernel. Fix DKMS first."

	grub2-reboot "$(boot_entry_of "${release}")"
	info "The next start uses ${release} once. The default stays ${old_default}."
	info "After the start, run ./install.sh kernel-dv again to make the patched kernel the default."
}

make_default_after_test_boot() {
	local release
	release="$(uname -r)"
	if ! bash "${component_dir}/check.sh"; then
		fail "The checks of the running kernel failed, so the default stays the stock kernel."
	fi
	grubby --set-default "/boot/vmlinuz-${release}"
	info "${release} is the default kernel now."
}

main() {
	install_packages "${component_dir}"
	[[ "$(distribution_family)" == fedora ]] || fail "kernel-dv supports only Fedora. See docs/distributions.md."
	install_file "${component_dir}/files/96-x86-linux-htpc-default.install" \
		/etc/kernel/install.d/96-x86-linux-htpc-default.install 0755

	local packages_dir
	packages_dir="$(config_require KERNEL_PACKAGES_DIR)"
	local listing
	listing="$(package_files "${packages_dir}")" || exit 1
	local -a files
	mapfile -t files <<<"${listing}"
	local release
	release="$(kernel_release_of "${files[@]}")"

	if ! rpm -q "kernel-core-${release%.*}" >/dev/null 2>&1; then
		install_and_boot_once "${files[@]}"
	elif [[ "$(uname -r)" == "${release}" ]]; then
		[[ "$(grubby --default-kernel)" == "/boot/vmlinuz-${release}" ]] || make_default_after_test_boot
	else
		info "${release} is installed. Start it once to test it, or run grub2-reboot $(boot_entry_of "${release}")."
	fi
}

main "$@"
