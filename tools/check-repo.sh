#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
#
# Checks the repository before a commit: shellcheck on all shell scripts,
# and a search for personal data that must not reach a public repository.
#
#   tools/check-repo.sh          check all files that git tracks or would add
#   tools/check-repo.sh FILE...  check only these files
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

failed=0

report() {
	printf '%s\n' "$*"
	failed=1
}

files_to_check() {
	if [[ $# -gt 0 ]]; then
		printf '%s\n' "$@"
	else
		git ls-files --cached --others --exclude-standard
	fi
}

is_shell_script() {
	[[ "$1" == *.sh ]] || head -n 1 "$1" 2>/dev/null | grep -qE '^#!.*\b(ba)?sh\b'
}

# License texts and the third-party skill are not ours to change.
is_excluded() {
	[[ "$1" == LICENSE || "$1" == LICENSES/* || "$1" == .agents/skills/simple-english/* ]]
}

# IPv4 addresses, except the ranges that RFC 5737 reserves for documentation,
# loopback, and four single digits. The last form is a CEC physical address
# such as 2.0.0.0, not a network address.
# 239.255.255.250 is the fixed multicast group of SSDP in the UPnP standard.
find_ipv4() {
	grep -noE '\b([0-9]{1,3}\.){3}[0-9]{1,3}\b' "$1" |
		grep -vE ':(192\.0\.2|198\.51\.100|203\.0\.113)\.[0-9]+$' |
		grep -vE ':127\.0\.0\.1$|:0\.0\.0\.0$' |
		grep -vE ':239\.255\.255\.250$' |
		grep -vE ':[0-9]\.[0-9]\.[0-9]\.[0-9]$' || true
}

# RFC 7042 keeps 00:00:5e:00:53:00 to 00:00:5e:00:53:ff for documentation.
# The documents are for the users of the software, so test records stay out:
# dates, and sentences about who saw a result in which test. The plan of the
# development is a record by design, and the skills describe how to test.
find_test_record() {
	case "$1" in
	README.md | AGENTS.md | docs/*.md) ;;
	*) return 0 ;;
	esac
	grep -noiE '\b20[0-9]{2}-[0-9]{2}-[0-9]{2}\b|\b(a|the) (person|human|tester) (saw|heard|confirmed|reported)|\b(confirmed|tested|seen) (at|on) the (TV|receiver)\b|\bin (a|the|our|my) test\b|\bthe test (machine|board|TV)\b|\bwe (tested|found|saw)\b' "$1" || true
}

find_mac() {
	grep -noiE '\b([0-9a-f]{2}:){5}[0-9a-f]{2}\b' "$1" |
		grep -viE ':00:00:5e:00:53:[0-9a-f]{2}$' || true
}

main() {
	local file hit
	local -a scripts=()
	while read -r file; do
		[[ -f "${file}" ]] || continue
		is_excluded "${file}" && continue
		if is_shell_script "${file}"; then
			scripts+=("${file}")
		fi
		while read -r hit; do
			[[ -n "${hit}" ]] && report "IP address in ${file}:${hit}"
		done < <(find_ipv4 "${file}")
		while read -r hit; do
			[[ -n "${hit}" ]] && report "MAC address in ${file}:${hit}"
		done < <(find_mac "${file}")
		while read -r hit; do
			[[ -n "${hit}" ]] && report "Test record in ${file}:${hit}. Write it as a fact, see the skill write-docs."
		done < <(find_test_record "${file}")
	done < <(files_to_check "$@")

	if [[ "${#scripts[@]}" -gt 0 ]]; then
		shellcheck "${scripts[@]}" || report "shellcheck found problems."
	fi

	if [[ "${failed}" -ne 0 ]]; then
		printf 'FAIL\n'
		return 1
	fi
	printf 'PASS\n'
}

main "$@"
