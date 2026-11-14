# Home Assistant

Home Assistant is a home automation server. It can turn the TV on and off, select the input of the AV receiver and start a video on the HTPC. The project installs nothing on the Home Assistant server. This page shows the parts that work with the HTPC. The skill `home-assistant` in `.agents/skills/` lets an AI agent build them in your Home Assistant.

## What talks to what

| Job | Way | Home Assistant part |
|---|---|---|
| Turn on the TV | Wake-on-LAN to the MAC addresses of the TV | The integration "Wake on LAN" |
| Turn off the TV | The network API of the TV | The integration of the TV brand |
| Select the input of the receiver | The network API of the receiver | The integration of the receiver brand |
| Start a video on the HTPC | The Jellyfin server sends a play command to the session of `jellyfin-mpv-shim` | A REST command to the Jellyfin API |
| Stop a video | The same API | A REST command |

The HTPC itself also wakes the TV: the component `network-wake` sends Wake-on-LAN at login and at the start of each video. So a cast from a phone turns on the TV without Home Assistant.

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

## Start a video on the HTPC

The Jellyfin server knows the HTPC as a session with the client "Jellyfin MPV Shim" and the name of `JELLYFIN_PLAYER_NAME`, or the host name. Home Assistant asks the server for this session and sends a play command to it.

1. In the Jellyfin dashboard, open API Keys and make a key for Home Assistant. Put the key into `secrets.yaml` of Home Assistant as `jellyfin_api_key`. Do not put the key into a file of this project.
2. Add two REST commands:

```yaml
# configuration.yaml
rest_command:
  jellyfin_htpc_play:
    url: "http://192.0.2.10:8096/Sessions/{{ session_id }}/Playing?playCommand=PlayNow&itemIds={{ item_id }}"
    method: post
    headers:
      Authorization: !secret jellyfin_authorization
  jellyfin_htpc_stop:
    url: "http://192.0.2.10:8096/Sessions/{{ session_id }}/Playing/Stop"
    method: post
    headers:
      Authorization: !secret jellyfin_authorization
```

The secret `jellyfin_authorization` has the value `MediaBrowser Token="KEY"`, with the API key in place of `KEY`.

3. Find the session ID with a GET request to `/Sessions` with the same header. At each start of `jellyfin-mpv-shim`, the session gets a new ID. So the script finds the ID before each play command. A REST sensor or a small script reads `/Sessions` and picks the entry with the right `DeviceName`.
4. Find the item ID of a film or an episode in the address of its page in the Jellyfin web client. A search request to `/Items?searchTerm=...` also gives it.

The play command starts the video at once. The HTPC then sends Wake-on-LAN, so the TV turns on also without the script of the first section.

## A full "watch" script

1. Turn on the receiver and select the input of the HTPC.
2. Send Wake-on-LAN to the TV.
3. Wait until the session of the HTPC answers on the Jellyfin server.
4. Send the play command.

A "screen off" script does the opposite: stop the video, turn off the TV with its integration, and turn off the receiver.

## Limits

1. YouTube, Twitch and the other apps have no remote API on the HTPC. Home Assistant can start them only through the phone apps, with a cast to VacuumTube for YouTube.
2. The HTPC has no API for its own power. Keep the HTPC on. An idle HTPC uses little power, and only a running HTPC receives a cast.
