# Instructions for agents

This repository builds a home theater PC (HTPC) on a normal x86 Linux system. `README.md` gives the goals. This file gives the rules for every agent that works here. The step-by-step procedures are skills in `.agents/skills/`.

## Start here

1. Read `README.md` for the goals and the list of documents.
2. Read `docs/status.md` for the state of each part.
3. Read the page in `docs/` for the part that your task is about.
4. Load the skill that matches your task. The table below lists them.

## Rules

1. The documents describe the current state. Git keeps the history. Do not write a changelog. Do not write text such as "was", "used to be", "previously" or "changed from" in a document or a comment.
2. A comment explains why the code does something. The code itself shows what it does. If the code needs a comment to explain what it does, change the code so that it is clear to a human reader.
3. Write all documents, comments, commit messages and messages to the user in simple English. Use the skill `simple-english`.
4. Keep the repository neutral. Do not write personal data into any file: no IP addresses, host names, user names, passwords, serial numbers or MAC addresses. Do not name the devices of one household. Describe hardware by its type or its feature, for example "an AV receiver with Dolby Atmos".
5. Keep the personal configuration out of the repository. The user or the agent of the user puts it on the target machine. The scripts read them from a configuration file outside the repository.
6. Put each document in the file for its topic. Do not grow one large file. If no file fits, add a new file and add it to the table in `README.md`.
7. Put each procedure into its own skill. Do not grow this file.
8. Write the documents for the users of the software, not as a record of your work. Test results go in as facts about the software and the hardware type, without dates, test steps, test devices or who saw the result. The skill `write-docs` has the details.

## Skills

The skills are in `.agents/skills/`, in the [Agent Skills](https://agentskills.io) format. An agent that supports this format compares the description of each skill with the task, and loads the matching skill by itself. Each description starts with "Use when". Agents without skill support read the skills as documents.

`.claude/skills` is a symbolic link to `.agents/skills`, because Claude Code reads project skills only from that folder. Edit the skills in `.agents/skills/`. Do not put files into `.claude/` or into another folder that only one agent reads.

| Skill | Use it to |
|---|---|
| `simple-english` | Write any text in this repository |
| `write-docs` | Add or change a document in `docs/` or a README |
| `write-code` | Add or change a script, a program or a configuration file |
| `add-component` | Add a new installable part under `components/` |
| `test-on-hardware` | Run tests on a target machine, and turn the result into facts in `docs/` |
| `kernel-patches` | Change, rebase or build the kernel patch set |
| `sync-upstream` | Take new versions of the external projects into this repository |
| `upgrade-stack` | Upgrade the kernel, FFmpeg, libplacebo, CB1 and mpv in the right order when a new kernel or a new libreelec-x86-DV commit is out |
| `update-cb1` | Take a new CB1 version into the build and the mpv patches |
| `setup-htpc` | Set up the HTPC on the machine of a user: ask the questions, run `setup.sh`, guide the steps at the TV |
| `home-assistant` | Build the TV and video scripts in the Home Assistant of a user |
| `test-setup` | Test `setup.sh` and the install on a fresh Fedora system in a container or a virtual machine |
| `android-app-settings` | Find a setting of an Android app in its APK, and make the component `android-apps` write it |
