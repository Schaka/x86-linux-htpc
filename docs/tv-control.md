# TV control and remote keys

The HTPC needs five jobs around the TV and the AV receiver. HDMI-CEC can do all of them with one USB adapter, but other ways exist for each job. This page lists them, so that each setup can pick the ways that fit its hardware. [cec.md](cec.md) describes the CEC way in detail.

## The jobs

| Job | Example |
|---|---|
| TV on | The user starts a video from a phone, and the TV turns on |
| Select the input of the HTPC | The TV and the receiver switch to the HTPC |
| TV and receiver off | The user ends the evening from the Bigscreen menu |
| Power state | Home automation knows whether the TV is on |
| Remote keys | The user moves through Bigscreen, the players and the apps |

A sixth job is control of the HTPC from home automation, for example "play this film on the TV". The last section of this page covers it.

## Ways for each job

| Job | HDMI-CEC | Network | Infrared | Other |
|---|---|---|---|---|
| TV on | Image View On | Wake-on-LAN. The TV must support a wake from standby over the network. | An IR transmitter sends the power code of the TV | At its own power on, the receiver turns on the TV over its own CEC |
| Select the input | Active Source | The network API of the receiver selects its input, and the TV stays on the input of the receiver | An IR transmitter sends the input code | |
| TV and receiver off | Standby | The network APIs of the TV and the receiver | An IR transmitter | At its own power off, the receiver turns off the TV over its own CEC |
| Power state | Give Device Power Status | The network API of the TV, or a ping | None | |
| Remote keys | The TV remote, through the TV | A phone app, for example KDE Connect | A USB IR receiver with any IR remote | A Bluetooth remote, or a 2.4 GHz remote with its own USB receiver |

## Air mouse remotes

A 2.4 GHz air mouse appears as four input devices: a keyboard, a mouse, a "System Control" device and a "Consumer Control" device. A common model sends these keys:

| Key on the remote | Key that Linux sees |
|---|---|
| Arrows | `KEY_UP`, `KEY_DOWN`, `KEY_LEFT`, `KEY_RIGHT` |
| OK | `BTN_LEFT`, a left mouse click |
| Back | `BTN_RIGHT`, a right mouse click |
| Home | `KEY_HOMEPAGE` |
| Menu | `KEY_COMPOSE` |
| Play/Pause, Previous, Next | `KEY_PLAYPAUSE`, `KEY_PREVIOUSSONG`, `KEY_NEXTSONG` |
| Volume and Mute | `KEY_VOLUMEUP`, `KEY_VOLUMEDOWN`, `KEY_MUTE` |
| Search | `KEY_SEARCH` |
| Power | `KEY_SLEEP` |

Many air mice have a mouse key that turns the motion sensor off and on. With the sensor off, the remote sends keys but no pointer movement.

Some air mice change OK themselves: with the sensor on, OK is `BTN_LEFT`, and with the sensor off, OK is `KEY_ENTER`. Back is `KEY_BACK`. Such a remote needs no entry in `AIR_MOUSE_USB_IDS`. [hardware.md](hardware.md#known-air-mice) lists known air mice and their keys. For another remote, run `tools/detect-remote.py` on the HTPC and press the keys in the order that it shows. It says whether the remote needs an entry in `AIR_MOUSE_USB_IDS`. While they lie still, some of these remotes also send a small movement about once a second.

The TV interfaces move a focus frame with the arrow keys, and there OK must be Enter. With the pointer, OK must be a click. The component `remote-keys` runs the service `x86-linux-htpc-air-mouse.service` for the receivers in `AIR_MOUSE_USB_IDS`. The service takes their input devices for itself and sends their events through one virtual device, "x86-linux-htpc air mouse":

| Key | What the service sends |
|---|---|
| OK, after a pointer movement in the last 5 seconds | `BTN_LEFT`, a click at the pointer |
| OK, else | `KEY_ENTER` for the item with the focus frame |
| An arrow key | The arrow key. It also ends the pointer use at once. For 2 seconds after an arrow key, OK is always Enter, and movement does not count, because the pointer of an air mouse also moves while the hand presses the keys. |
| Back | `KEY_BACK` |
| All other keys and the movement | Unchanged |

A hand never holds the remote still, so a movement counts only above a small distance. The CB1 mpv on its own VT has no window system. Its script `htpc-screen.run` gives mpv the pointer movement and the clicks, and draws a pointer on the OSD. Only a movement above the same small distance shows the pointer. The pointer goes away 5 seconds after the last such movement, or at an arrow key. While the pointer is off, the script gives mpv no movement, because mpv shows the controls of the player at each mouse movement.

The component `bigscreen` makes the sleep key do nothing, so the power key of such a remote does not suspend the HTPC. A suspended HTPC takes no cast, and some boards do not wake up from a USB remote.

## Network control

Most smart TVs and receivers have a network API, and home automation systems have integrations for them. The network way needs no extra hardware and no free HDMI input.

1. Turn on: the TV must accept Wake-on-LAN in standby. Most TVs have a setting for it, often under names such as "turn on via Wi-Fi" or "networked standby". Some TVs wake only over Wi-Fi and not over the cable, so list both MAC addresses.
2. Input: if the TV always shows the input of the receiver, the receiver input is the only switch. The receiver API selects it.
3. Off and state: the TV API turns the TV off and reports its state.

The network way does not carry remote keys. Combine it with a remote from the next section.

The component `network-wake` does the first step. It sends a Wake-on-LAN packet to each address in `WAKE_ON_LAN_MACS`:

| When | How |
|---|---|
| At login | The user service `htpc-wake.service` |
| At the start of each video in `jellyfin-mpv-shim` | The mpv script `htpc-wake.lua`, for both mpv programs. A cast then turns on a TV in standby. |
| At the start of each video in VacuumTube | The launcher of the component `youtube-tv`. A cast from the YouTube app of a phone then turns on a TV in standby. |

The packet goes to the broadcast address of the LAN on the UDP ports 9 and 7, three times. A device that is on ignores it. The receiver and the input stay with the home automation system, because each brand has its own API.

## Remote keys without CEC

| Remote | What the HTPC needs | Notes |
|---|---|---|
| 2.4 GHz remote with its own USB receiver, often sold as an "air mouse" | A free USB port | No pairing. Linux sees a keyboard and a mouse. Many have a microphone and a pointer. |
| Bluetooth remote | Bluetooth. A board without Bluetooth needs a USB Bluetooth adapter. | Make sure that it works as a standard keyboard (HID) and not only with one TV brand |
| IR remote with a USB IR receiver | A free USB port | A FLIRC receiver learns the codes of any IR remote and sends keyboard keys, without a driver. Other receivers use the kernel IR framework (`ir-keytable`). The remote of the receiver can work for both devices. |
| Phone | Network | KDE Connect sends keys and media commands to Plasma. The Jellyfin apps control `jellyfin-mpv-shim` through the server. |

An IR remote of the TV itself can also change the TV, so use free keys or a remote of another device.

## Control from home automation

| Target | Way |
|---|---|
| `jellyfin-mpv-shim` | The sessions API of the Jellyfin server: play an item, pause, stop and seek |
| Android apps in Waydroid | An SSH command on the HTPC, for example `waydroid app launch` or `waydroid shell am start` with the link of a video |
| Keys, media commands and screen state | An agent on the HTPC that connects to home automation over MQTT, for example LNXlink |
| CEC commands | An SSH command with `cec-ctl`. It needs a CEC adapter on the HTPC. |

[home-assistant.md](home-assistant.md) shows the scripts for Home Assistant.

## Choose a combination

The project supports the second combination: [status.md](status.md) has the details. Some examples of complete combinations:

| Combination | Hardware | Limits |
|---|---|---|
| CEC for all jobs | A USB-CEC adapter | Inline in the HDMI cable, the adapter limits the bandwidth. [hardware.md](hardware.md) explains this. |
| Network for power and input, a 2.4 GHz remote for keys | A USB remote | The TV must wake over the network |
| Network for power and input, an IR receiver for keys | A USB IR receiver | The same |
| Network for power and input, CEC for keys | A USB-CEC adapter | Inline in the HDMI cable, the same bandwidth limit as the first row |
