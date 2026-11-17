# Power key

If the devices are off, the power key of a remote turns the TV and the AV receiver on. If they are on, it turns them off. A suspended HTPC takes no cast, so the HTPC itself stays on. The component `power-key` does this job. [tv-control.md](tv-control.md) lists the other ways to control the TV.

## Choose a route

| | Route 1: Home Assistant | Route 2: the HTPC alone |
|---|---|---|
| Who does the work | Home Assistant, through a webhook | The HTPC, with Wake-on-LAN and commands |
| State of the devices | Home Assistant knows it | A command of yours reads it |
| You need | A Home Assistant server with integrations for the TV and the receiver | A network command for each device. [device-commands.md](device-commands.md) shows how to find them. |
| Key setting | `POWER_WEBHOOK_URL` | `POWER_OFF_COMMAND`, `POWER_ON_COMMAND` and `POWER_STATE_COMMAND` |

The route follows from `POWER_WEBHOOK_URL`. If the key is empty, the HTPC uses route 2. Both routes can exist at the same time, and one value of the configuration file chooses.

## How it works

1. The service `x86-linux-htpc-power-key.service` reads the keys of all input devices. It ignores the power button and the sleep button of the computer.
2. At a key from `POWER_KEYS`, the service runs `htpc-power toggle` as the user of the TV session. One press starts one run. A second press during the run, or within 1.5 seconds, does nothing.
3. `htpc-power` turns on what is off, and off what is on, by one of the two routes.

The air mouse service of the component `remote-keys` hands the key on through its own virtual device. So the service sees the key of an air mouse too.

## Keys

The power key of an air mouse sends `KEY_SLEEP`. Some remotes send `KEY_POWER`. `tools/detect-remote.py` shows what a remote sends. `POWER_KEYS` takes the names of all keys that turn the devices on and off.

The system ignores `KEY_POWER` and `KEY_SLEEP`. Another key, such as `KEY_SEARCH`, also acts in the desktop: Plasma Bigscreen opens its search. So for such a key, the service takes the input device of the remote and passes all other events on through a virtual device. The name of that device ends with "(x86-linux-htpc power key)". The service takes no keyboard. A device with letter keys keeps all its keys. The air mouse device of the component `remote-keys` is the one exception, because it has the keys of a keyboard but belongs to a remote.

The HTPC never suspends and never shuts down by a key. The component `bigscreen` makes logind and Plasma ignore the power key, the sleep key, the lid and the idle time. It also masks the sleep targets. If that configuration is missing, the install of `power-key` stops. The power button of the board does nothing to the HTPC, and it does not turn the TV on or off.

## Route 1: Home Assistant

Set `POWER_WEBHOOK_URL` to a webhook of Home Assistant. The service sends `{"action": "toggle"}` to it with an HTTP POST. Home Assistant knows the state of the devices, so it decides what to do.

For a toggle, the HTPC first sends Wake-on-LAN to the addresses in `WAKE_ON_LAN_MACS`. A TV that Home Assistant cannot turn on then still wakes. A device that is on ignores the packet. `POWER_WEBHOOK_WAKE=no` turns this step off.

The ID in the address works like a password, so choose a long random ID. The logs of the HTPC do not show the address. [home-assistant.md](home-assistant.md) shows the other scripts that this automation can use.

```yaml
automation:
  - alias: Power key of the HTPC remote
    mode: single
    triggers:
      - trigger: webhook
        webhook_id: "A-LONG-RANDOM-ID"
        allowed_methods: [POST]
        local_only: true
    actions:
      - choose:
          - conditions:
              - condition: state
                entity_id: media_player.tv
                state: "on"
            sequence:
              - action: rest_command.htpc_control
                data:
                  path: home
                continue_on_error: true
              - action: media_player.turn_off
                target:
                  entity_id: media_player.tv
                continue_on_error: true
              - action: media_player.turn_off
                target:
                  entity_id: media_player.receiver
        default:
          - action: media_player.turn_on
            target:
              entity_id: media_player.receiver
          - delay: "00:00:03"
          - action: media_player.select_source
            target:
              entity_id: media_player.receiver
            data:
              source: "HTPC input"
```

The entity names and the input name are examples. The webhook address is `http://HOME_ASSISTANT:8123/api/webhook/A-LONG-RANDOM-ID`. The TV needs its own off action, for example the integration of the brand. The packet of the HTPC turns on the TV.

To remove this route, delete the automation and empty `POWER_WEBHOOK_URL`.

## Route 2: the HTPC alone

Without a webhook, the HTPC does the work. This route needs no Home Assistant and no extra hardware.

| Step | What happens |
|---|---|
| On | The HTPC sends Wake-on-LAN to the addresses in `WAKE_ON_LAN_MACS`, then runs `POWER_ON_COMMAND`. Use `WAKE_ON_LAN_MACS` for the TV, and the command for the receiver. |
| Off | The HTPC stops all media through the control API, then runs `POWER_OFF_COMMAND`. Put the off commands of the TV and the receiver into it. |
| State | The HTPC runs `POWER_STATE_COMMAND`. The exit code 0 means on. |

A packet on the network can only turn a device on, so the off step needs a command. Each brand has its own network protocol. [device-commands.md](device-commands.md) shows how to find the commands, and lists examples for common brands.

The commands run as the user of the TV session, not as root. The configuration file takes one line for a command. For a longer sequence, write a script and put its path into the value.

### The state

Without `POWER_STATE_COMMAND`, the devices count as on while the HDMI output has a connected display. If the TV and the receiver drop the HDMI link in standby, this works. A receiver can keep the link up in standby. Then the state is always "on", and the key only turns off. So read the state from the receiver with `POWER_STATE_COMMAND`. [device-commands.md](device-commands.md) has an example.

If the state is not known, the key turns the devices on. An extra Wake-on-LAN packet does no harm.

## The command line

`htpc-power` is the program behind the key. To try a route without the remote, run it on the HTPC:

| Command | Job |
|---|---|
| `htpc-power toggle` | Does what the key does |
| `htpc-power on`, `htpc-power off` | Turn on or off, without a look at the state |
| `htpc-power state` | Prints `on`, `off` or `unknown`. The exit code is 0, 1 or 2. |

Home automation can run these commands over SSH, as [tv-control.md](tv-control.md) describes.

## Configuration

| Key | Meaning |
|---|---|
| `POWER_KEYS` | The evdev names of the keys. The default is `KEY_SLEEP`. Empty: the service does not run. |
| `POWER_WEBHOOK_URL` | Route 1. Empty: route 2. |
| `POWER_WEBHOOK_WAKE` | `yes` sends Wake-on-LAN before the webhook in route 1. The default is `yes`. |
| `POWER_ON_COMMAND`, `POWER_OFF_COMMAND` | The shell commands of route 2 |
| `POWER_STATE_COMMAND` | The command that tells the state in route 2 |
| `POWER_STOP_MEDIA` | `yes` stops all media before the off step of route 2. It needs the component `control-api`. |

After a change of `POWER_KEYS`, run `sudo ./install.sh power-key`. The other keys apply at the next key press.

## Limits

1. A remote that sends its power button by infrared does not reach the service. Only a key that the remote sends over USB works. For such a remote, put another key into `POWER_KEYS`, for example `KEY_SEARCH`.
2. The off step of route 2 needs a command for the network protocol of the device. The project has no built-in driver for a brand.
3. A TV needs a few seconds to enter standby. A second press in that time can miss the TV, so wait a few seconds between presses.
4. After the TV turns on, Plasma needs a moment to show the picture again.
