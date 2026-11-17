# Home Assistant

Home Assistant is a home automation server. It can turn the TV on and off, select the input of the AV receiver and start a video on the HTPC. The project installs nothing on the Home Assistant server. This page shows the parts that work with the HTPC. The component `control-api` gives Home Assistant a media player entity and a REST API for the HTPC. The skill `home-assistant` in `.agents/skills/` lets an AI agent build them in your Home Assistant.

## What talks to what

| Job | Way | Home Assistant part |
|---|---|---|
| Turn on the TV | Wake-on-LAN to the MAC addresses of the TV | The integration "Wake on LAN" |
| Turn off the TV | The network API of the TV | The integration of the TV brand |
| Select the input of the receiver | The network API of the receiver | The integration of the receiver brand |
| Start a YouTube video, a Jellyfin item or an app on the HTPC | The REST API of the component `control-api` | A REST command |
| See what plays, pause, stop, seek | The DLNA renderer of the component `control-api` | The integration "DLNA Digital Media Renderer" |

The HTPC itself also wakes the TV: the component `network-wake` sends Wake-on-LAN at login and at the start of each video. So a start from Home Assistant or a cast from a phone turns on the TV without a step in the script.

## Turn the TV on

The TV must accept Wake-on-LAN in standby. Many TVs have an option for it, for example "Turn on via Wi-Fi" or "network standby". The TV listens on the MAC address of the network connection that it used last. So send the packet to the cable and the Wi-Fi address.

```yaml
# configuration.yaml
wake_on_lan:
```

```yaml
# A script
- action: wake_on_lan.send_magic_packet
  data:
    mac: "00:00:5e:00:53:01"
- action: wake_on_lan.send_magic_packet
  data:
    mac: "00:00:5e:00:53:02"
```

Use the same addresses as `WAKE_ON_LAN_MACS` in `/etc/x86-linux-htpc/htpc.conf`.

## Turn the TV off

Use the action `media_player.turn_off` on the entity of the TV integration. Many TVs drop off the network in standby, so the integration cannot turn them on. The packet of the section above does that job.

If the receiver turns on and off with the TV, the TV and the receiver use HDMI-CEC with each other. The HTPC needs no CEC for that.

## The HTPC as a media player

The component `control-api` runs a DLNA renderer. Home Assistant finds it by itself: Settings > Devices & services shows a discovered "DLNA Digital Media Renderer" with the name of the HTPC. If Home Assistant runs without host networking and finds nothing, add the integration by hand with the address `http://HTPC:8780/dlna/device.xml`.

The entity shows the title, the image and the position of the media that plays, in jellyfin-mpv-shim, VacuumTube, Kodi or mpv. It also shows a cast from a phone. Its controls are play, pause, stop and seek. It has no volume, because the AV receiver has the volume.

`media_player.play_media` on the entity plays a media address: a YouTube address in VacuumTube, a stream of the Jellyfin server in the shim, and each other address in mpv. [control-api.md](control-api.md) has the rules.

## Start a video on the HTPC

The REST API of `control-api` starts media by its ID, with the right player and the resume position. Add one REST command for all its paths:

```yaml
# configuration.yaml
rest_command:
  htpc_control:
    url: "http://192.0.2.20:8780/api/v1/{{ path }}"
    method: post
    content_type: application/json
    payload: "{{ (body | default({})) | tojson }}"
    timeout: 60
```

The timeout is long, because a start into Kodi waits until Kodi runs.

```yaml
# Play a YouTube video in VacuumTube
- action: rest_command.htpc_control
  data:
    path: play/youtube
    body:
      video_id: "aqz-KE-bpKQ"

# Play a Jellyfin item at its resume position, in the shim or in Kodi
- action: rest_command.htpc_control
  data:
    path: play/jellyfin
    body:
      item_id: "{{ item_id }}"
      player: shim

# Start the Twitch app
- action: rest_command.htpc_control
  data:
    path: apps/s0undtv/launch
```

`GET /api/v1/apps` lists the app IDs of the home screen. [api.md](api.md) describes all paths. The answer of a start is in the response variable: status `202` means that the player starts the media. Other codes come with a sentence in `content.message`.

Find the item ID of a film or an episode in the address of its page in the Jellyfin web client. A search request to `/Items?searchTerm=...` of the Jellyfin server also gives it. A voice assistant can search with its own tools and give the ID to the script.

### Without the component control-api

The Jellyfin server can send a play command to the session of `jellyfin-mpv-shim`. This needs a Jellyfin API key in `secrets.yaml` of Home Assistant, as `jellyfin_authorization` with the value `MediaBrowser Token="KEY"`.

```yaml
rest_command:
  jellyfin_htpc_play:
    url: "http://192.0.2.10:8096/Sessions/{{ session_id }}/Playing?playCommand=PlayNow&itemIds={{ item_id }}"
    method: post
    headers:
      Authorization: !secret jellyfin_authorization
```

The session gets a new ID at each start of the shim. A GET request to `/Sessions` with the same header lists the sessions. The script picks the entry with the right `DeviceName`, the value of `JELLYFIN_PLAYER_NAME` or the host name.

## The power key of the remote

The component `power-key` can send the power key of the remote to a webhook of Home Assistant. An automation then turns the TV and the receiver on or off by their state, and the HTPC stays on. [power-key.md](power-key.md) shows the automation.

## A full "watch" script

1. Turn on the receiver and select the input of the HTPC.
2. Send Wake-on-LAN to the TV.
3. Start the media through `rest_command.htpc_control`.

A "screen off" script does the opposite: `rest_command.htpc_control` with the path `home` stops all media, then the script turns off the TV with its integration and turns off the receiver.

## Limits

1. The Android apps have no remote API. Home Assistant can start them through `apps/ID/launch`, but cannot read or control what they play.
2. The HTPC has no API for its own power. Keep the HTPC on. An idle HTPC uses little power, and only a running HTPC receives a cast.
