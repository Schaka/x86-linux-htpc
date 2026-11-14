---
name: write-code
description: Use when you add or change a script, a program, a patch or a configuration file in this repository. Examples are the install.sh and check.sh of a component, a file in lib/, an mpv script and a systemd unit.
---

# Write code

## Rules

1. Write for a human reader first. Use clear names for functions and variables, and short functions with one job each.
2. A comment explains why the code does something: a hardware limit, a bug in another project, a choice between two ways. The code shows what it does. If the code needs a comment to explain what it does, change the code instead.
3. Do not write comments about the history, for example "changed from X" or "this used to be". Git keeps the history.
4. Write comments in simple English. Load the skill `simple-english`.
5. Write no personal data into code or configuration: no addresses, host names, user names or passwords. Read such values from `/etc/x86-linux-htpc/htpc.conf` through the functions in `lib/`.
6. Keep distribution-specific code in `lib/` and in the package lists of the components. [docs/distributions.md](../../../docs/distributions.md) has the rules.
7. Make install scripts safe to run again. A second run changes nothing on a system that is already correct.
8. Do not change a file that belongs to a package of the distribution. Add a drop-in file instead.

## Shell scripts

1. Use Bash. Start each script with `#!/usr/bin/env bash` and `set -euo pipefail`.
2. Put the license header `# SPDX-License-Identifier: AGPL-3.0-only` on the second line.
3. Quote all variables.
4. Use the message functions of `lib/` instead of `echo` for messages to the user.
5. Make sure that `shellcheck` reports no problems.

## Other languages

1. Python: use the standard library first. For a dependency, use a package of the distribution.
2. Lua for mpv scripts.
3. A patch to another project keeps the license of that project. Put its SPDX header into the patch description.

## Before you finish

1. Run `shellcheck` on each changed shell script.
2. If `tools/check-repo.sh` exists, run it.
3. If you have a test machine, run the `check.sh` of each changed component on it. Otherwise say in the commit message that the change is not tested on hardware.
