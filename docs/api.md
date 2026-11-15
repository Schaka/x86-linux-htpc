# REST API reference

This page describes each path of the REST API of the component `control-api`. [control-api.md](control-api.md) describes the service, its install and the DLNA renderer.

The OpenAPI document is the exact description of the API. Each release has it, and this address always gives the one of the newest release:

```
https://github.com/Schaka/x86-linux-htpc/releases/latest/download/openapi.yaml
```

The service on the HTPC also serves the document of its own version at `http://HTPC:8780/openapi.json`.

## Rules for all paths

1. The base address is `http://HTPC:8780`, with the address of the HTPC and `CONTROL_API_PORT`.
2. The API has no authentication. It is for the local network only.
3. A request body is JSON, with the header `Content-Type: application/json`.
4. Times and positions are seconds.
5. A start answers `202`: the player starts the media. The media can take some seconds until it plays. `GET /api/v1/status` shows the moment it plays.
6. A failed request answers with an error body:

```json
{
  "error": "nothing_plays",
  "message": "No player plays media."
}
```

| Status | `error` | Meaning |
|---|---|---|
| `400` | `bad_request` | A value in the request is wrong, for example a video ID with the wrong length |
| `404` | `not_found` | No app or Jellyfin item has this name or ID |
| `409` | `nothing_plays` | A playback command, but no player plays media |
| `502` | `player_error` | The player or the Jellyfin server refused the command |
| `503` | `not_installed` | The player or the component for the request is not installed |
| `503` | `offline` | The player does not run, or jellyfin-mpv-shim has no Jellyfin login |

---

## Status

### `GET /health`

Shows that the service runs, and its version.

```json
{
  "status": "ok",
  "version": "1.0.0-beta5"
}
```

---

### `GET /api/v1/status`

Shows the media that plays, its player and its position. The service reads the players every 2 seconds.

| Field | Type | Description |
|---|---|---|
| `source` | string | The player: `idle`, `jellyfin`, `youtube`, `kodi` or `url` (the mpv of the service) |
| `state` | string | `idle`, `playing`, `paused` or `buffering` |
| `title` | string or null | The title of the movie, episode or video |
| `subtitle` | string or null | The series with season and episode, or the YouTube channel |
| `position` | number or null | Seconds from the start |
| `duration` | number or null | Seconds |
| `media_id` | string or null | The Jellyfin item ID or the YouTube video ID |
| `url` | string or null | The media address. For YouTube, the watch address. |
| `image` | string or null | An image of the media |
| `running.youtube` | boolean | VacuumTube runs, also without a video |
| `running.kodi` | boolean | Kodi runs, also without media |

```bash
curl http://HTPC:8780/api/v1/status
```

```json
{
  "source": "jellyfin",
  "state": "playing",
  "title": "Operation Strix",
  "subtitle": "Spy x Family, S1 E1",
  "position": 601.2,
  "duration": 1480.1,
  "media_id": "0123456789abcdef0123456789abcdef",
  "url": null,
  "image": "http://jellyfin.example.org/Items/0123456789abcdef0123456789abcdef/Images/Primary",
  "running": { "youtube": false, "kodi": false }
}
```

---

## Play

### `POST /api/v1/play/youtube`

Plays a YouTube video in VacuumTube. Give `video_id` or `url`.

| Field | Type | Default | Description |
|---|---|---|---|
| `video_id` | string | | The 11 characters of the video ID |
| `url` | string | | A YouTube address: `youtube.com/watch?v=`, `youtu.be/`, `/shorts/` or `/live/` |
| `start` | number | | The start in seconds |

```bash
curl -X POST http://HTPC:8780/api/v1/play/youtube \
  -H 'Content-Type: application/json' -d '{"video_id": "aqz-KE-bpKQ", "start": 30}'
```

```json
{ "source": "youtube" }
```

---

### `POST /api/v1/play/jellyfin`

Plays a movie, an episode or a song of the Jellyfin server of jellyfin-mpv-shim.

| Field | Type | Default | Description |
|---|---|---|---|
| `item_id` | string | | The Jellyfin item ID. The address of the item page in the Jellyfin web client contains it. |
| `player` | string | `shim` | `shim` for jellyfin-mpv-shim, `kodi` for Kodi with the add-on "Jellyfin for Kodi". If Kodi does not run, the service starts it. |
| `start` | number | | The start in seconds |
| `resume` | boolean | `true` | Without `start`: `true` starts at the resume position of the Jellyfin user of the player, `false` at the beginning. The shim and the Kodi add-on each have their own login. |

```bash
curl -X POST http://HTPC:8780/api/v1/play/jellyfin \
  -H 'Content-Type: application/json' -d '{"item_id": "0123456789abcdef0123456789abcdef"}'
```

```json
{ "source": "jellyfin" }
```

With `"player": "kodi"`, the answer is `{"source": "kodi"}`.

---

### `POST /api/v1/play/url`

Plays a media address in the player that fits it. A YouTube address plays in VacuumTube. A stream address of the Jellyfin server of the shim plays the item in the shim. Each other address plays in mpv, with the options of the shim. An audio address plays in mpv without a picture. The schemes `http`, `https`, `rtsp`, `rtmp`, `smb` and `nfs` are allowed.

| Field | Type | Default | Description |
|---|---|---|---|
| `url` | string | | The media address |
| `title` | string | | The title that mpv shows and reports |
| `audio` | boolean | `false` | `true` for an audio address without a known file extension. The service decides by the extension for `mp3`, `flac`, `m4a`, `aac`, `ogg`, `oga`, `opus`, `wav`, `wma`, `aiff` and `alac`. |

```bash
curl -X POST http://HTPC:8780/api/v1/play/url \
  -H 'Content-Type: application/json' -d '{"url": "https://example.org/film.mkv", "title": "Film"}'
```

```json
{ "source": "url" }
```

---

## Playback

These paths act on the player that plays, as `GET /api/v1/status` shows it. Without media, the answer is `409` with `nothing_plays`. A success answers `204` without a body.

| Path | Body | Job |
|---|---|---|
| `POST /api/v1/playback/pause` | | Pauses |
| `POST /api/v1/playback/resume` | | Continues |
| `POST /api/v1/playback/toggle` | | Pauses or continues |
| `POST /api/v1/playback/stop` | | Stops the media. Kodi and VacuumTube keep running. |
| `POST /api/v1/playback/seek` | `{"position": 600}` | Goes to the position, in seconds from the start |

```bash
curl -X POST http://HTPC:8780/api/v1/playback/seek \
  -H 'Content-Type: application/json' -d '{"position": 600}'
```

---

## Apps

### `GET /api/v1/apps`

Lists the apps of the home screen: the desktop entries of `HOME_SCREEN_APPS`, or else the entries of the components.

```json
[
  { "id": "jellyfin", "name": "Jellyfin", "desktop_id": "com.github.iwalton3.jellyfin-mpv-shim.desktop" },
  { "id": "youtube", "name": "YouTube", "desktop_id": "htpc-youtube-tv.desktop" },
  { "id": "kodi", "name": "Kodi", "desktop_id": "htpc-kodi.desktop" },
  { "id": "s0undtv", "name": "Twitch", "desktop_id": "htpc-android-s0undtv.desktop" }
]
```

---

### `POST /api/v1/apps/{id}/launch`

Starts an app, by its `id` or its `desktop_id`. The other players stop their media first. The answer is the app.

```bash
curl -X POST http://HTPC:8780/api/v1/apps/s0undtv/launch
```

```json
{ "id": "s0undtv", "name": "Twitch", "desktop_id": "htpc-android-s0undtv.desktop" }
```

---

## Device

### `POST /api/v1/home`

Stops all media, ends Kodi and VacuumTube, and minimizes the other app windows, so the home screen shows. The Android apps keep running in the background. The answer is `204`.

---

### `POST /api/v1/tv/wake`

Sends Wake-on-LAN to the MAC addresses of `WAKE_ON_LAN_MACS`. A success answers `204`. Without the component `network-wake`, the answer is `503` with `not_installed`.
