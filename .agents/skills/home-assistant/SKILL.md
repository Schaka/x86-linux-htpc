---
name: home-assistant
description: Use when a user wants Home Assistant to control the TV, the receiver input or the media on this HTPC. You build the scripts in the Home Assistant of the user, as docs/home-assistant.md describes.
---

# Home Assistant for the HTPC

[docs/home-assistant.md](../../../docs/home-assistant.md) describes the parts. You build them in the Home Assistant of the user. Nothing goes into this repository.

## Rules

1. The Home Assistant of the user is not yours. Before each change, show it to the user and ask.
2. Ask the user for a long-lived access token of Home Assistant. Keep it in an environment variable of your shell or in the secret store of the user. Never print it, and never write it into a file of this repository. The component `control-api` needs no Jellyfin API key. Only the way without it needs one.
3. Find out how the user keeps the configuration of Home Assistant: in the web interface, in YAML files, or in a git repository. Follow that way.
4. Before you turn off the TV, ask. Someone can be watching it.

## Steps

1. Ask for the address of Home Assistant. Read the states with `GET /api/states`, and find the TV, the receiver and the "Wake on LAN" integration.
2. Read `WAKE_ON_LAN_MACS`, `JELLYFIN_SERVER_URL`, `JELLYFIN_PLAYER_NAME` and `CONTROL_API_PORT` from `/etc/x86-linux-htpc/htpc.conf` on the HTPC.
3. Make sure that the component `control-api` runs: `curl http://HTPC:8780/health`. If it does not, offer `sudo ./install.sh control-api`.
4. Look for the discovered "DLNA Digital Media Renderer" of the HTPC in the discovery flows (WebSocket `config_entries/flow/progress`). Add it, or start the flow of `dlna_dmr` and pick the HTPC. Give the entity a clear name and the area of the TV.
5. Ask the user which jobs to build, for example TV on, a YouTube video, a Jellyfin item or a "watch" script. Ask which receiver input the HTPC uses.
6. Build each job as [docs/home-assistant.md](../../../docs/home-assistant.md) shows, with `rest_command.htpc_control` for the starts. Use the entity IDs of the user. If the user has scripts for another player, add new scripts next to them. Do not change the old scripts until the user agrees.
7. Test the chain without a picture first: a start with a wrong ID answers 400 or 404 through Home Assistant.
8. Test each script once, after the user agrees. For a video, use a short item, and read `GET /api/v1/status` and the state of the media player entity. Ask the user what the TV shows.
9. Write down for the user which scripts exist and what each one does.

## If the TV has no network API

Some TVs drop off the network in standby, and some have no integration. Then Wake-on-LAN is the only network way to turn the TV on. The TV turns off with its own remote, with HDMI-CEC from the receiver, or with the sleep timer of the TV.
