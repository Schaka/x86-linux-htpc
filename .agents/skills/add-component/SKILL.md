---
name: add-component
description: Use when you add a new installable part under components/, or when you change the install, check or remove script of a component.
---

# Add a component

A component is one installable part of the HTPC, for example `codecs`, `bigscreen` or `cec`.

## Steps

1. Pick a short name in lower case with hyphens. The name says what the part does. If the product behind the part can change, do not use the product name. For example `audio-passthrough`, not `wireplumber-config`.
2. Make the folder `components/<name>/` with these files:

   | File | Job |
   |---|---|
   | `install.sh` | Installs the part. A second run changes nothing. |
   | `check.sh` | One `PASS` or `FAIL` line for each check. The exit code is 0 for all passed checks, and 1 for a failed check. |
   | `remove.sh` | Removes the part and its files |
   | `packages/fedora.txt` | The Fedora packages, one per line |
   | `files/` | Files that `install.sh` copies to the system |

3. Use the functions of `lib/` for messages, packages, files and the configuration file.
4. If the component needs a value from the user, add a key with a comment and a neutral example value to `htpc.conf.example`.
5. Add the name to `components/order.txt` after the components that it needs.
6. Describe the component in its page in `docs/`. Say what it installs and which configuration keys it reads.
7. Load the skill `write-code` and follow its rules.

## Checks

A check tests one fact on the running system, for example "the HDMI node has the codec list" or "the service runs". A check does not repeat the install steps. A check that needs a person, for example "the TV shows Dolby Vision", belongs in [docs/testing.md](../../../docs/testing.md), not in `check.sh`.
