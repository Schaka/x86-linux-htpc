# HDMI-CEC

The project has no CEC component yet. This page is its design. Network wake and a USB remote do the same jobs today, as [tv-control.md](tv-control.md) describes.

HDMI-CEC lets the devices on HDMI control each other. It is one of the ways in [tv-control.md](tv-control.md). The HTPC uses it for three jobs: it turns the TV and the receiver on, it turns them off, and it reads the keys of the TV remote. The HTPC uses a Pulse-Eight USB-CEC adapter, because the HDMI port of most GPUs has no CEC controller. [hardware.md](hardware.md) explains the adapter.

## Two ways to drive the adapter

| Way | How it works | Who can use CEC |
|---|---|---|
| Kernel driver | The kernel module `pulse8-cec` drives the adapter. The tool `inputattach` connects the serial port of the adapter to the module. The device `/dev/cec0` appears. | All programs at the same time, through `/dev/cec0`. `cec-ctl` sends commands. |
| libcec | libcec opens the serial port of the adapter directly | One program at a time |

The project uses the kernel driver. With it, the kernel turns the remote keys into normal key presses (`CONFIG_MEDIA_CEC_RC`). Plasma, mpv, Kodi and Waydroid get the keys like keys from a keyboard. No program needs to own the adapter.

If its libcec contains the Linux CEC back end, Kodi can use `/dev/cec0` too. If it does not, Kodi gets the remote keys as key presses and does not send CEC commands itself.

## Physical address

Each HDMI device has a physical address, for example `2.0.0.0`. It shows where the device sits in the HDMI chain. The TV and the receiver use it to switch to the input of a device.

The adapter sits on a free HDMI input, not on the input of the HTPC. So it must report the address of the HTPC, not the address of its own port. The CEC service reads the EDID (the description that the TV sends over HDMI) of the HDMI connector of the GPU. This EDID contains the address of the HTPC input. The service gives this address to the adapter with `cec-ctl --phys-addr-from-edid`.

## CEC service

A systemd service starts `inputattach` for the adapter, registers the HTPC as a playback device, and sets the physical address. Small commands for the other parts use `cec-ctl`:

| Event | CEC messages |
|---|---|
| The HTPC starts or wakes up | Image View On to the TV, then Active Source with the address of the HTPC |
| A player starts playback | Active Source, so the TV and the receiver switch to the HTPC |
| The user turns the system off from Plasma Bigscreen | Standby to all devices |

The configuration file `/etc/x86-linux-htpc/htpc.conf` turns each event on or off.

## Control from other systems

A home automation system can control the HTPC over the network. It connects with SSH and runs `cec-ctl`, or it uses the API of the player. The repository does not contain the configuration of such a system.
