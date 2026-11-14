---
name: write-docs
description: Use when you add, change or move a document in this repository. Examples are a page in docs/, a README and the text of a skill.
---

# Write documents

## Rules

1. Write in simple English. Load the skill `simple-english` and follow it.
2. Describe the current state only. Git keeps the history. Do not use words such as "was", "used to", "previously", "now", "new" or "changed from" about the project itself.
3. Do not add a changelog, a history section or release notes.
4. Write for the people who install and use the software. A document says how the software works, what it needs, what it supports and what its limits are. It is not a record of the work.
5. Keep test records out of the documents. Write no dates and no test steps. Do not write who saw a result, or how a problem was found. Do not name the machines, TVs or receivers of a test. Turn a test result into a fact about the software and the hardware type:

   | Do not write | Write |
   |---|---|
   | In a test, the TV showed 4K at 60 Hz. | The CB1 mpv plays 4K at 60 Hz on an N100 class GPU. |
   | A person saw the logo at the TV. | The TV gets Dolby Vision. |
6. Write no personal data: no IP addresses, host names, user names, passwords, MAC addresses or serial numbers. Do not name the devices of one household. Name hardware by its type or feature, for example "a TV with Dolby Vision".
7. Put each topic into its own file. Before you write, find the file for the topic in the table of `README.md`. If no file fits, add a new file and a row in that table.
8. Keep one fact in one place. Other files link to it.
9. Use relative links between documents.

## Where each kind of text belongs

| Text | File |
|---|---|
| How a part works and why | The page in `docs/` for that part |
| What the hardware must do, and the facts about known hardware types | `docs/hardware.md` |
| What each feature supports, and its limits | `docs/status.md` |
| Work that is not done, and the status of work | `docs/status.md` and the issue tracker of the repository |
| How to do a task step by step, for agents | A skill in `.agents/skills/` |
| A rule for every task of every agent | `AGENTS.md` |

## Steps

1. Read the file that you change, and the files that link to it.
2. Make the change.
3. If the change makes a fact in another file wrong, change that file too.
4. If you added a file, add it to the table in `README.md`.
5. Run the self-check of the skill `simple-english` on each changed file.
6. Run `tools/check-repo.sh`. It also finds dates and test wording in the documents.
