# Commands for TVs and AV receivers

The power key can work without Home Assistant. Then the HTPC needs a command for each device. This page shows how to find these commands, and gives examples for common brands. [power-key.md](power-key.md) describes the key itself.

## What the route needs

| Step | TV | AV receiver |
|---|---|---|
| On | Wake-on-LAN. List the MAC addresses in `WAKE_ON_LAN_MACS`. | A command. Many receivers ignore Wake-on-LAN. |
| Input | None. The receiver selects it for the TV. | A command that selects the input of the HTPC |
| Off | A command | A command |
| State | Not needed | A command. Its exit code 0 means on. |

A receiver in standby can keep the HDMI link up, so the HDMI state alone is not enough. Standby of a receiver does not turn off a TV that has no HDMI-CEC link to it. So the TV needs its own off command.

## Find the commands for a device

1. Give the device a fixed address. Use a static lease in the router.
2. Turn on the network control setting of the device. The name differs: "network standby", "networked standby", "remote start", "IP control" or "Mobile TV On".
3. Find the protocol. The page of the brand in the Home Assistant documentation names the protocol and the library. The source of that library shows the exact messages.
4. Scan the ports with `nmap -p- ADDRESS`. Scan the device once in the on state, and once in standby. A port that is open in standby can also turn the device on.
5. Send a message that only reads, for example a power state query. Make sure that the answer is correct.
6. Ask the people in the room, then send an off command.
7. Put the commands into `/etc/x86-linux-htpc/htpc.conf`, and run `htpc-power state`, `htpc-power off` and `htpc-power on` on the HTPC.

The commands run as the user of the TV session, not as root. Keep files with keys in the home folder of that user, with the mode `600`. A value in the configuration file cannot contain a double quote. A command with a double quote goes into a script, and the value holds the path of the script.

## Ports and protocols

The list shows the protocols of common brands. "Works" means that the commands below work on a device of that kind. "Not tried" means that the project has no device of that kind. For the other rows, the protocol comes from community documentation or from the documentation of the brand. Make sure that it works on your own device.

| Device | Protocol | Port | Status |
|---|---|---|---|
| LG webOS TV | WebSocket with a pairing key | 3000, 3001 | Works |
| Denon and Marantz receiver | Text commands over TCP | 23 | Works |
| Samsung TV with Tizen | WebSocket with a token | 8001, 8002 | Not tried |
| Hisense TV with VIDAA | MQTT with a key message | 36669 | Not tried |
| Sony Bravia TV | HTTP JSON with a pre-shared key | 80 | Not tried |
| Android TV or Google TV | Android Debug Bridge | 5555 | Not tried |
| Roku TV | HTTP (ECP) | 8060 | Not tried |
| Yamaha receiver | HTTP (Extended Control) | 80 | Not tried |
| Onkyo, Pioneer and Integra receiver | eISCP over TCP | 60128 | Not tried |

## LG webOS TV

The TV wakes by Wake-on-LAN. Turn on its network wake setting, for example "Turn on via Wi-Fi", and list its MAC addresses. A TV with a cable and Wi-Fi has two addresses, and it listens on the one that it used last.

The TV turns off through its network API. The tool `bscpylgtv` sends the command. Install it in a folder of the HTPC user, and pair it once:

```
python3 -m venv /home/htpc/.local/share/lgtv
/home/htpc/.local/share/lgtv/bin/pip install bscpylgtv
/home/htpc/.local/share/lgtv/bin/bscpylgtvcommand -p /home/htpc/.local/share/lgtv/keys.sqlite 192.0.2.40 get_current_app
chmod 600 /home/htpc/.local/share/lgtv/keys.sqlite
```

The first command makes the TV show a prompt. Accept it with the remote of the TV. The answer is the app that runs now. The file `keys.sqlite` holds the pairing key. The off command is `power_off`:

```
POWER_OFF_COMMAND=/home/htpc/.local/share/lgtv/bin/bscpylgtvcommand --log WARNING -p /home/htpc/.local/share/lgtv/keys.sqlite 192.0.2.40 power_off
```

Replace `htpc` with the user of the TV session, and the address with the address of the TV.

## Denon and Marantz receivers

These receivers take text commands over TCP port 23, each ended by a carriage return. Denon and Marantz publish a protocol document for each model. It lists all commands. The HTTP page of some models only redirects, so use the port 23.

| Command | Meaning |
|---|---|
| `PWON` | On. The next command follows 1 second later. |
| `PWSTANDBY` | Standby |
| `PW?` | State. The answer is `PWON` or `PWSTANDBY`. |
| `SIMPLAY` | Input "Media Player". The codes of the inputs differ by model. |

The tool `nc` (`ncat`) closes the connection at the end of its input, and the answer comes later. So keep the input open for one second:

```
POWER_STATE_COMMAND=(printf 'PW?\r'; sleep 1) | nc -w 2 192.0.2.30 23 | grep -q PWON
POWER_ON_COMMAND=(printf 'PWON\r'; sleep 1) | nc -w 2 192.0.2.30 23; sleep 3; (printf 'SIMPLAY\r'; sleep 1) | nc -w 2 192.0.2.30 23
POWER_OFF_COMMAND=(printf 'PWSTANDBY\r'; sleep 1) | nc -w 2 192.0.2.30 23
```

If the TV has its own off command, put it first in `POWER_OFF_COMMAND`. Join it to the receiver command with a semicolon.

## Other brands

These notes come from the documentation of the brands and from community projects. The project did not try them.

| Device | How to find the commands |
|---|---|
| Samsung TV with Tizen | The TV shows a prompt at the first connection, and gives a token. The library `samsungtvws` sends keys. The key `KEY_POWER` turns the power on and off. Reports say that some models only turn off with it, so use Wake-on-LAN to turn on. |
| Hisense TV with VIDAA | The TV runs an MQTT broker on the port 36669. Community projects send the key `KEY_POWER` to it. Hisense does not document this, and support depends on the model. A Hisense TV with Roku or Google TV uses the protocol of that system. |
| Sony Bravia TV | The REST API takes a JSON request on `/sony/system`. Set a pre-shared key in the network menu of the TV, and send it in the header `X-Auth-PSK`. The method is `setPowerStatus`. The TV must have "Remote start" on to turn on. The Sony page "setPowerStatus" shows the request. |
| Android TV or Google TV | Turn on the developer options and the network debugging. Then `adb connect ADDRESS:5555` and `adb shell input keyevent KEYCODE_SLEEP` turn the TV off. `KEYCODE_WAKEUP` turns it on. |
| Roku TV | `curl -d '' http://ADDRESS:8060/keypress/PowerOff` turns the TV off. Turn on the setting "Control by mobile apps". |
| Yamaha receiver | `GET /YamahaExtendedControl/v1/main/setPower?power=standby` turns off, and `power=on` turns on. `getStatus` in the same folder shows the state. Turn on "Network Standby" for the on step. |
| Onkyo, Pioneer and Integra receiver | The eISCP protocol adds a binary header to the text command. The commands are `PWR01` for on and `PWR00` for off. Use a library, for example `python-eiscp`. Turn on the network standby setting of the receiver. |

The JSON of the Sony request has double quotes, so write a script:

```
#!/usr/bin/env bash
curl -fsS -H 'X-Auth-PSK: YOUR-KEY' \
  -d '{"method":"setPowerStatus","id":1,"params":[{"status":false}],"version":"1.0"}' \
  http://192.0.2.50/sony/system
```

Then set `POWER_OFF_COMMAND` to the path of this script.
