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
| `CONTROL_DIAL` | `yes` | `no` turns the DIAL launcher for VacuumTube off |
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

Without `start`, the item starts at the resume position of the Jellyfin user of the player. The shim and the Kodi add-on each have their own login, and they can use different Jellyfin users. The service reads the login of the add-on from its `data.json`, because the add-on does not resume by itself. `"resume": false` starts at the beginning.

The shim has no local API. It is a session on the Jellyfin server, and the server sends it the play commands. The service uses the login of the shim from `~/.config/jellyfin-mpv-shim/users.json`, so it needs no API key. Its requests use the token of the shim, so the server counts them as the shim and shows no second session.

### Which player plays a media address

`POST /api/v1/play/url` and the DLNA renderer use the same rules:

1. A YouTube address (`youtube.com/watch`, `youtu.be`, `/shorts/`, `/live/`) plays in VacuumTube.
2. A stream address of the Jellyfin server of the shim (`/Videos/ID/...`, `/Items/ID/...`, `/Audio/ID/...`) plays the item in the shim. Jellyfin then keeps the watched state and the resume position.
3. An audio address plays in the mpv of the distribution, without a picture and without the configuration of the shim. The TV stays on Plasma, and the TV wake does not run. The address is audio when it has the extension of an audio file, when the API request says `audio`, or when the DLNA item has the class `object.item.audioItem`. A video address that follows ends the audio mpv first.
4. Each other address plays in mpv. mpv uses the configuration folder of the shim, so it uses the same mpv program, options and scripts. With `JELLYFIN_MPV=cb1`, this is the CB1 mpv on its own VT with Dolby Vision. The FFmpeg of the CB1 build has GnuTLS for `https://` addresses. When this mpv ends, its script `htpc-screen.run` gives the screen back to Plasma, as for the shim.

### One player at a time

Kodi and the CB1 mpv each take the whole screen on their own VT. So a start first stops the media of the other players. It ends Kodi, unless Kodi plays next, and it pauses VacuumTube. The Android apps of Waydroid keep running in the background.

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

## DIAL launcher for VacuumTube

DIAL is the protocol that the YouTube app of a phone uses to find a TV. VacuumTube answers it by itself, but only while it runs. The service answers for VacuumTube while the app is closed, so the HTPC shows up in the cast list of the YouTube app at all times. The service runs the launcher only when the entry of the YouTube app exists.

| Step | What happens |
|---|---|
| Discovery | The service answers the SSDP search for `urn:dial-multiscreen-org:service:dial:1`. It is silent while VacuumTube runs, because VacuumTube answers then, and the phone must see the HTPC once. |
| Launch | The phone sends `POST /dial/apps/YouTube` with its pairing data. The service answers `201` at once, starts VacuumTube, and sends the request on to the DIAL server of VacuumTube when this server takes it. Other players make room first, as for a video. |
| While VacuumTube runs | The service sends `GET` and `POST` requests of a phone that still uses its address to the DIAL server of VacuumTube. |

A phone shows a DIAL device in the cast list of the YouTube app only after it linked the device once, for example while VacuumTube runs, or with the TV code in the settings of VacuumTube. Then the phone lists the HTPC under "Linked devices", also while VacuumTube is closed. This differs from a Google Cast device, which the phone lists without a link. Google Cast needs a licensed receiver, so the HTPC cannot be one.

The service answers with the name and the UDN (the unique ID) of VacuumTube, so the phone sees one device, whether VacuumTube runs or not. VacuumTube keeps its UDN over restarts. The service notices each start of VacuumTube, reads both from its description, and keeps them in `~/.local/state/x86-linux-htpc/dial-identity.json`. Before that first run, the service uses the name of the HTPC with the suffix " (VacuumTube)" and an own UDN. When the UDN changes, the service sends an SSDP goodbye for the old UDN, so a phone forgets the old device at once. A phone that linked the device before can list the device twice for a short time.

The path has the name of the app as the DIAL registry spells it, `YouTube`. The phone sends this name. The service compares the name without regard to case. The REST API uses lower case names.

The service finds the DIAL server of VacuumTube with an SSDP search, because the server uses another port at each start. So VacuumTube needs its option `device_discoverability`, which is on by default. The service cannot stop VacuumTube over DIAL, and it does not need to.

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
