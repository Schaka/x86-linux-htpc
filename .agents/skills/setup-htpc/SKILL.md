---
name: setup-htpc
description: Use when a user asks you to set up, install or reconfigure this HTPC on their machine. You ask the setup questions in the chat, run setup.sh, and guide the user through the steps that need a person. When the user wants Home Assistant or CEC, also use it.
---

# Set up the HTPC

You set up the HTPC on the machine of the user. `setup.sh` does the work. Your job is to ask the questions in plain words, explain each choice, write the answers file and watch the install. [docs/install.md](../../../docs/install.md) describes the same steps for a person.

## Rules

1. Before you reboot the machine, install a kernel or change the boot loader, ask the user.
2. Ask before you play a sound or a video on the TV. Someone can be in the room.
3. Never print a password, a token or an API key. The setup needs none of them. The Jellyfin login uses Quick Connect.
4. Write no address, user name or MAC address of the user into a file of the repository. The answers file goes to `/var/tmp/htpc-answers.conf`.
5. If a step fails, show the error to the user, and fix the cause. Do not skip a failed component in silence.

## 1. Check the machine

1. Read `/etc/os-release`. The setup supports Fedora with KDE Plasma. On another distribution, tell the user that only Fedora has package lists, and point to [docs/distributions.md](../../../docs/distributions.md). Stop there, unless the user wants you to add the distribution.
2. Make sure that the display manager is Plasma Login Manager or SDDM: `systemctl show -p Id --value display-manager.service`. The command prints nothing while no display manager is turned on. GDM does not work.
3. Make sure that the machine reaches the internet. The setup downloads from Fedora, RPM Fusion, Flathub and GitHub.
4. Make sure that the user can run `sudo`.
5. Run `./setup.sh --questions` for the current list of questions, and `./setup.sh --defaults` for the values that the setup detects on this machine. Use these two outputs, not a list from memory.

## 2. Ask the questions

Ask in small groups. Give the detected default and a recommendation with each question. Tell the user that Enter or "default" takes the default.

| Group | Keys | What to tell the user |
|---|---|---|
| Screen and user | `HTPC_USER`, `HDMI_OUTPUT`, `DISPLAY_MODE` | The detected values come from the TV. A TV that is off can report no modes, so ask the user to turn it on first. |
| Sound | `SETUP_AV_RECEIVER`, `AUDIO_DRIVER` | With an AV receiver, all formats go to it unchanged. Without one, the TV gets Dolby Digital and Dolby Digital Plus. Keep the detected driver. |
| Jellyfin | `SETUP_JELLYFIN`, `JELLYFIN_SERVER_URL`, `JELLYFIN_PLAYER_NAME` | Ask for the address of the server. Try it with `curl -fsS URL/System/Info/Public`, and tell the user the server name from the answer. No password and no API key are needed. |
| Dolby Vision and Kodi | `SETUP_DOLBY_VISION`, `SETUP_KODI`, `SETUP_BUILD` | Dolby Vision needs an Intel GPU, a TV with Dolby Vision and a patched kernel. The builds take hours. Recommend `no` for a first install on unknown hardware. |
| Apps | `SETUP_YOUTUBE`, `ANDROID_APPS`, `TWITCH_PROXY_URL` | VacuumTube plays YouTube with HDR. Waydroid runs SmartTube and S0undTV for Twitch, in SDR. Keep the default proxy of S0undTV. |
| Remote | `SETUP_REMOTE`, `AIR_MOUSE_USB_IDS` | Ask which remote the user has. For an air mouse, show the detected USB IDs and the names from `/sys/bus/usb/devices/*/product`, and let the user pick the receiver. |
| Comfort | `SCREEN_EDGES` | Recommend `off`. A pointer in a screen corner then does not open the overview of KWin. |
| TV power | `WAKE_ON_LAN_MACS` | Ask for the MAC addresses of the TV, cable and Wi-Fi. If the user knows only the IP address of the TV, ping it and read `ip neigh show IP`. Remind the user to turn on the network wake option of the TV. |
| Remote control | `SETUP_CONTROL` | The control API lets Home Assistant and phones start media on the HTPC. It has no authentication and is for the local network. Recommend `yes`. |
| Extras | `SETUP_HOME_ASSISTANT`, `SETUP_CEC` | See the last section. |

## 3. Run the setup

1. Write the answers to `/var/tmp/htpc-answers.conf`, one `KEY=value` line each. Put a value with spaces in double quotes. Leave out a key to take its default. Show the file to the user before you go on.
2. Check the answers without an install: `sudo ./setup.sh --answers /var/tmp/htpc-answers.conf --no-install`. Show the user the list of components and the home screen apps.
3. Run the install in the background, with a log: `sudo ./setup.sh --answers /var/tmp/htpc-answers.conf 2>&1 | tee /var/tmp/htpc-setup.log`. It takes 10 to 30 minutes without the builds, and hours with them. Read the log from time to time, and tell the user what the setup does.
4. After an error or a failed check, the exit code is 1. With all checks passed, it is 0. At the end, the setup prints `PASS` and `FAIL` lines for each component and the next steps.
5. On the first run, four kinds of checks fail. These are the Jellyfin login, the user service of `jellyfin-mpv-shim`, the home screen parts that need the session, and the groups of the user. The steps below fix them. Each other `FAIL` line is a real problem. Show it to the user, and fix the cause.

## 4. The steps with the user

1. Jellyfin login: run `sudo components/jellyfin-mpv-shim/login.sh`. Give the user the code that it prints, and tell the user to approve it within 5 minutes in another Jellyfin client: user menu, Quick Connect.
2. Restart: ask the user, then restart. After the restart, ask what the TV shows. It shows Plasma Bigscreen with the tiles of the answers.
3. Run `sudo ./install.sh home-screen`. Now the session runs, and the component hides the other rows and binds the Home key.
4. With Dolby Vision, the patched kernel starts once as a test. If the system works, run `sudo ./install.sh kernel-dv`, and the kernel becomes the default.
5. Ask the user to sign in to YouTube and the Android apps on the TV.
6. With Kodi, set up its Jellyfin add-on as the section "Jellyfin in Kodi" of [docs/players.md](../../../docs/players.md) describes. Kodi answers JSON-RPC on `localhost:9090` while it runs, so you can read its dialogs with `GUI.GetProperties` and press keys with `Input.Down` and `Input.Select`. Give the user the Quick Connect code, and ask the user which libraries to sync.
7. Run `sudo ./install.sh --check`, and go through each failed check with the user.

## Home Assistant

If the user has Home Assistant, load the skill `home-assistant`. It builds the TV and video scripts in the Home Assistant of the user.

## CEC

The project has no CEC component yet. [docs/cec.md](../../../docs/cec.md) describes the design for a Pulse-Eight USB-CEC adapter. Tell the user that network wake covers the power-on, and a USB remote covers the keys. If the user has an adapter and wants CEC, build the component as [docs/cec.md](../../../docs/cec.md) describes, with the skill `add-component`. Test it with the user at the TV, and with the skill `test-on-hardware`.
