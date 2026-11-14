# Control API

The component `control-api` runs `htpc-control`, a small service on the HTPC. It has two interfaces on one TCP port, 8780 by default.

| Interface | For | What it does |
|---|---|---|
| REST API under `/api/v1` | Home Assistant scripts and other programs | Starts a YouTube video, a Jellyfin item, a media address or an app. Controls the player that plays. Reports what plays. |
| DLNA renderer | The DLNA integration of Home Assistant and the DLNA apps of phones | Takes a media address and plays it in the player that fits it. Reports the state of the player that plays. |

The source code is in `control/`. It is one static program for x86-64 Linux, without libraries of the system.

## Install

`sudo ./install.sh control-api` installs the program to `/usr/local/libexec/x86-linux-htpc/htpc-control` and the user service `htpc-control.service` of the Plasma session. The service starts apps in the session, so it runs as the HTPC user and not as root.

The program comes from one of two places:

1. If `CONTROL_BUILD_DIR` (default `/var/tmp/htpc-control`) has the file `htpc-control`, the component installs it. `control/build.sh` builds it there.
2. Otherwise the component downloads `htpc-control-x86_64-linux` of the newest release from GitHub, and checks its SHA-256 sum.

If firewalld runs, the component opens the TCP port and SSDP in the default zone.

| Key in `htpc.conf` | Default | Meaning |
|---|---|---|
| `CONTROL_API_PORT` | `8780` | The TCP port of the REST API and of the DLNA descriptions |
| `CONTROL_DLNA` | `yes` | `no` turns the DLNA renderer off |
| `CONTROL_NAME` | `JELLYFIN_PLAYER_NAME`, else the host name | The name in Home Assistant and in the DLNA apps |
| `CONTROL_BUILD_DIR` | `/var/tmp/htpc-control` | The folder of a local build |

The service reads the file at its start. After a change, run `sudo ./install.sh control-api` again, which restarts the service.

## Security

The API and the DLNA renderer have no authentication. Each host of the local network can start media on the HTPC and stop it. This is the same as for DLNA renderers, Chromecast devices and smart TVs. Do not forward the port to the internet.

## REST API

[api.md](api.md) describes each path with its fields, examples and answers. The OpenAPI document is the exact description. The service serves it at `http://HTPC:8780/openapi.json`. Each release has it as `openapi.yaml`, at `https://github.com/Schaka/x86-linux-htpc/releases/latest/download/openapi.yaml`. `htpc-control openapi` prints it.

| Path | Body | Job |
|---|---|---|
| `GET /health` | | The service runs, with its version |
| `GET /api/v1/status` | | What plays: player, state, title, position, duration, image |
| `GET /api/v1/apps` | | The apps of the home screen, with their `id` |
| `POST /api/v1/apps/{id}/launch` | | Starts an app, for example `youtube`, `kodi`, `jellyfin` or `s0undtv` |
| `POST /api/v1/play/youtube` | `{"video_id": "...", "start": 30}` or `{"url": "https://youtu.be/..."}` | Plays a video in VacuumTube |
| `POST /api/v1/play/jellyfin` | `{"item_id": "...", "player": "shim"}` | Plays a movie, an episode or a song |
| `POST /api/v1/play/url` | `{"url": "...", "title": "..."}` | Plays a media address in the player that fits it |
| `POST /api/v1/playback/pause`, `resume`, `toggle`, `stop` | | Controls the player that plays |
| `POST /api/v1/playback/seek` | `{"position": 600}` | Goes to a position in seconds |
| `POST /api/v1/home` | | Stops all media, ends Kodi and VacuumTube, and minimizes the other app windows, so the home screen shows |
| `POST /api/v1/tv/wake` | | Sends Wake-on-LAN to `WAKE_ON_LAN_MACS`. Needs the component `network-wake`. |

A start answers `202` with the player, for example `{"source": "youtube"}`. A control answers `204`. An error answers with a code and a sentence, for example `409` and `{"error": "nothing_plays", "message": "No player plays media."}`.

```bash
curl -X POST http://HTPC:8780/api/v1/play/youtube \
  -H 'Content-Type: application/json' -d '{"video_id": "aqz-KE-bpKQ"}'
curl http://HTPC:8780/api/v1/status
```

### Jellyfin

`player` selects the player of a Jellyfin item.

| `player` | Player |
|---|---|
| `shim` (default) | `jellyfin-mpv-shim` |
| `kodi` | Kodi with the add-on "Jellyfin for Kodi". If Kodi does not run, the service starts it first. |

Without `start`, the item starts at the resume position of the Jellyfin user of the shim, in both players. `"resume": false` starts at the beginning. The Kodi add-on uses its own login, so log it in with the same Jellyfin user.

The shim has no local API. It is a session on the Jellyfin server, and the server sends it the play commands. The service uses the login of the shim from `~/.config/jellyfin-mpv-shim/users.json`, so it needs no API key. Its requests use the token of the shim, so the server counts them as the shim and shows no second session.

### Which player plays a media address

`POST /api/v1/play/url` and the DLNA renderer use the same rules:

1. A YouTube address (`youtube.com/watch`, `youtu.be`, `/shorts/`, `/live/`) plays in VacuumTube.
2. A stream address of the Jellyfin server of the shim (`/Videos/ID/...`, `/Items/ID/...`, `/Audio/ID/...`) plays the item in the shim. Jellyfin then keeps the watched state and the resume position.
3. Each other address plays in mpv. mpv uses the configuration folder of the shim, so it uses the same mpv program, options and scripts. With `JELLYFIN_MPV=cb1`, this is the CB1 mpv on its own VT with Dolby Vision. The FFmpeg of the CB1 build has GnuTLS for `https://` addresses. When the mpv of the service ends, the service makes sure that the screen goes back to Plasma.

### One player at a time

Kodi and the CB1 mpv each take the whole screen on their own VT. So a start first stops the media of the other players. It ends Kodi, unless Kodi plays next, and it pauses VacuumTube. The Android apps of Waydroid keep running in the background.

A start also wakes the TV (component `network-wake`). The AV receiver then syncs the HDMI signal again, and the HDMI audio device goes away for a moment. mpv and YouTube pause at this loss. So for 30 seconds after the start of a Jellyfin item, the service continues a paused video. The YouTube launcher does the same for 20 seconds.

## DLNA renderer

The renderer is a UPnP AV media renderer (DLNA DMR). It announces itself with SSDP on the local network. The Home Assistant integration "DLNA Digital Media Renderer" finds it and makes a media player entity.

| Feature | Support |
|---|---|
| Play a media address | Yes, with the rules above |
| Play, pause, stop, seek | Yes, on the player that plays |
| State, title, position, duration, image | Yes, of the player that plays, also for media that another app started, for example a cast from a phone |
| Volume and mute | No. With passthrough, the AV receiver has the volume. |
| Queue, next and previous | No |

Home Assistant shows the media card of the entity. Its media browser can send media from other integrations to it, for example from the Jellyfin integration. Music Assistant also finds the renderer and offers it as a player.

## Build

`control/build.sh [OUTPUT_DIR]` builds the program and writes `openapi.yaml` next to it. With cargo on the machine, it needs the target `x86_64-unknown-linux-musl`. Without cargo, it builds in an Alpine container with podman or docker.

The workflow `.github/workflows/control.yml` checks the format, runs clippy and the tests, and builds the program for each change in `control/`. Each run keeps the program, `openapi.yaml` and their SHA-256 sums as build artifacts. A tag `v*` also attaches them to its GitHub release, where the component downloads them.

## Limits

1. The service reads the players every 2 seconds, so the status can be up to 2 seconds old.
2. The Android apps have no remote API. The service starts them, but cannot read or control what they play.
3. YouTube plays only in VacuumTube, not in SmartTube.
4. The service reads VacuumTube through the debugging port of Chromium on localhost (port 47811), which the component `youtube-tv` turns on.
5. The service reaches Kodi through its JSON-RPC interface on `localhost:9090`.
6. The home screen of Plasma Bigscreen reports no state. So `home` minimizes the app windows instead of a toggle of the home screen. The launcher of the Android apps restores the minimized window of Waydroid.
