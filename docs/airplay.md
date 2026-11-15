# AirPlay

The component `airplay` runs UxPlay, an open AirPlay receiver. An iPhone, an iPad or a Mac on the same network then plays audio on the HTPC and mirrors its screen on the TV. The receiver appears in the AirPlay list of the Apple device, under the name in `AIRPLAY_NAME`.

## What it supports

| Feature | Support |
|---|---|
| Mirror the screen of an iPhone, iPad or Mac | Yes, in a full-screen window over Plasma |
| Play audio from an Apple device | Yes. The audio goes to PipeWire. |
| Senders with the older AirPlay 1 protocol, for example `pyatv` or older Android apps | No. UxPlay refuses them. |
| Video from apps that use AirPlay video, for example YouTube | Not turned on. UxPlay has the option `-hls` for it. |
| Protected content, for example Netflix or Apple TV+ | No. The apps block mirroring and AirPlay video of this content. |
| Android phones | No. Use the DLNA renderer of the [control API](control-api.md). |

UxPlay shows the picture with a GStreamer pipeline. On an Intel GPU, the `va` plugin decodes H.264 in hardware.

## Install

The component installs the package `uxplay` and the user service `htpc-airplay.service` of the Plasma session. The file `~/.config/x86-linux-htpc/uxplay.conf` holds the options of UxPlay. The service starts with the Plasma session and restarts after an error.

| Key | Meaning |
|---|---|
| `AIRPLAY_NAME` | The name in the AirPlay list. Empty: the host name. |

The component opens the fixed UxPlay ports (TCP 7000, 7001 and 7100, and UDP 6000, 6001 and 7011) and mDNS in the default zone of the firewall. The Apple device finds the receiver with mDNS (Bonjour).

## Limits

1. The receiver has no PIN. Each device on the local network can mirror to the TV. Do not put the HTPC into a network with devices that you do not trust.
2. The mirroring window is a Wayland window of Plasma. It does not appear over the CB1 mpv or Kodi, which take the whole screen on their own VT. [players.md](players.md) describes this.
3. A mirrored picture is SDR. UxPlay gets no HDR or Dolby Vision from the sender.
