---
name: home-assistant
description: Use when a user wants Home Assistant to switch the TV, select the receiver input, or start a Jellyfin video on this HTPC. You build the scripts in the Home Assistant of the user, as docs/home-assistant.md describes.
---

# Home Assistant for the HTPC

[docs/home-assistant.md](../../../docs/home-assistant.md) describes the parts. You build them in the Home Assistant of the user. Nothing goes into this repository.

## Rules

1. The Home Assistant of the user is not yours. Before each change, show it to the user and ask.
2. Ask the user for a long-lived access token of Home Assistant, and for a Jellyfin API key. Keep both in environment variables of your shell or in the secret store of the user. Never print them, and never write them into a file of this repository.
3. Find out how the user keeps the configuration of Home Assistant: in the web interface, in YAML files, or in a git repository. Follow that way.
4. Before you turn off the TV, ask. Someone can be watching it.

## Steps

1. Ask for the address of Home Assistant. Read the states with `GET /api/states`, and find the TV, the receiver and the "Wake on LAN" integration.
2. Read `WAKE_ON_LAN_MACS`, `JELLYFIN_SERVER_URL` and `JELLYFIN_PLAYER_NAME` from `/etc/x86-linux-htpc/htpc.conf` on the HTPC.
3. Ask the user which jobs to build: TV on, TV off, receiver input, a video on the HTPC, a "watch" script and a "screen off" script.
4. Build each job as [docs/home-assistant.md](../../../docs/home-assistant.md) shows. Use the entity IDs of the user.
5. Test each script once, after the user agrees. For a video, use a short item and ask the user what the TV shows.
6. Write down for the user which scripts exist and what each one does.

## If the TV has no network API

Some TVs drop off the network in standby, and some have no integration. Then Wake-on-LAN is the only network way to turn the TV on. The TV turns off with its own remote, with HDMI-CEC from the receiver, or with the sleep timer of the TV.
