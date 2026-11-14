# Install

This page sets up the HTPC on a fresh install of Fedora. `setup.sh` asks the questions, writes the configuration file, builds the large parts and installs the components. An AI agent can do the same steps with the skill `setup-htpc` in `.agents/skills/`. The agent then asks you the questions in the chat.

## Before you start

1. Install the Fedora KDE Plasma Desktop. Plasma Bigscreen needs Plasma 6 and its display manager, Plasma Login Manager or SDDM. The Fedora Workstation with GNOME and GDM does not work.
2. Make a normal user for the TV session during the install, for example `htpc`. The user needs no administrator rights.
3. Connect the HTPC to the TV, or to the AV receiver, and turn them on. The setup reads the HDMI port and the display modes from the TV.
4. Connect the remote. For an air mouse, plug in its USB receiver.
5. Get the address of your Jellyfin server, for example `http://192.0.2.10:8096`. The HTPC logs in with Quick Connect, so it needs no password and no API key.
6. Connect the HTPC to the internet. The setup downloads packages from Fedora and RPM Fusion, apps from Flathub and GitHub, and the sources of the builds.
7. For Wake-on-LAN, turn on the network wake option of the TV. Get the MAC addresses of the TV from its network menu, or from your router.

[hardware.md](hardware.md) lists the requirements for each part. Dolby Vision needs an Intel GPU, the patched kernel and the CB1 build.

## Run the setup

Get the repository and start the setup as root:

```
git clone https://github.com/OWNER/x86-linux-htpc
cd x86-linux-htpc
sudo ./setup.sh
```

The setup shows a default for each question. Press Enter to take it. `./setup.sh --questions` lists all questions before you start.

| Question | What it decides |
|---|---|
| `HTPC_USER` | The user of the TV session. The default is the user that started `sudo`. |
| `HDMI_OUTPUT`, `DISPLAY_MODE` | The HDMI port and the mode of the desktop. The setup reads both from the TV. |
| `AUDIO_DRIVER` | `hda` for machines with SOF audio. The setup finds SOF by itself. [audio.md](audio.md) explains it. |
| `SETUP_AV_RECEIVER` | `yes` sends all audio formats unchanged. `no` sends only Dolby Digital and Dolby Digital Plus to the TV. |
| `SETUP_JELLYFIN`, `JELLYFIN_SERVER_URL` | The Jellyfin player `jellyfin-mpv-shim` and its server |
| `SETUP_DOLBY_VISION` | The patched kernel and the CB1 mpv, which plays all videos on its own VT |
| `SETUP_KODI` | Kodi with the CB1 engine as a second player |
| `SETUP_YOUTUBE` | VacuumTube, the TV interface of YouTube |
| `ANDROID_APPS`, `TWITCH_PROXY_URL` | Android TV apps in Waydroid, for example SmartTube and S0undTV for Twitch |
| `SETUP_REMOTE`, `AIR_MOUSE_USB_IDS` | `air-mouse` for a remote that sends OK and Back as mouse buttons. `keyboard` for all other remotes, keyboards and mice. |
| `SCREEN_EDGES` | `off` removes the action of the screen corners. Without it, a pointer in a corner opens the overview of KWin. |
| `WAKE_ON_LAN_MACS` | The TV turns on at login and at each video |
| `SETUP_BUILD` | Build the CB1, Kodi and kernel packages now |
| `SETUP_HOME_ASSISTANT`, `SETUP_CEC` | The setup then names the guides at the end |

The setup writes `/etc/x86-linux-htpc/htpc.conf`. At the end, it runs the checks of all components and shows the next steps. On the first run, some checks fail, because the Jellyfin login and the first session of the TV user do not exist yet. Then `setup.sh` ends with the exit code 1. The next steps fix these checks. The key `COMPONENTS` in this file lists the components that the answers need. `htpc.conf.example` explains each key.

## The builds for Dolby Vision and Kodi

Dolby Vision and Kodi need three builds. With `SETUP_BUILD=yes`, the setup runs them in containers with `podman`:

| Build | Script | Result | Time on a 4-core computer |
|---|---|---|---|
| FFmpeg, libplacebo, CB1 and mpv | `cb1/build-in-container.sh` | `x86-linux-htpc-cb1-fcN.tar.gz` in `CB1_BUILD_DIR` | about 30 minutes |
| Kodi | `cb1/build-in-container.sh` | `x86-linux-htpc-kodi-fcN.tar.gz` in `CB1_BUILD_DIR` | about 1.5 hours |
| Kernel | `kernel/build-in-container.sh` | RPM packages in `KERNEL_PACKAGES_DIR` | several hours |

A faster computer with the same Fedora release can run the builds. Copy the results into the folders on the HTPC, and answer `SETUP_BUILD=no`.

## After the setup

1. Log the HTPC in to Jellyfin: `sudo components/jellyfin-mpv-shim/login.sh`. The script shows a code. Approve the code within 5 minutes in another Jellyfin client: user menu, Quick Connect.
2. Restart the computer. The TV then shows Plasma Bigscreen.
3. Run `sudo ./install.sh home-screen`. Before the first login, the setup writes only the list of tiles. The running session also hides the other rows of the home screen and binds the Home key.
4. With Dolby Vision, the patched kernel starts once as a test. If the system works, run `sudo ./install.sh kernel-dv`, and the kernel stays the default.
5. Sign in to the apps on the TV, for example YouTube in VacuumTube.
6. Run `sudo ./install.sh --check` to check all components. A machine without a TV on HDMI fails three checks: the display mode, the HDMI audio and the hardware decode. A virtual machine is an example.

## Change the setup later

Run `sudo ./setup.sh` again, or change `/etc/x86-linux-htpc/htpc.conf` and run `sudo ./install.sh`. A second run changes only what is different. `sudo ./install.sh --remove NAME` removes one component.

## Home Assistant and CEC

The setup installs nothing for these two. [home-assistant.md](home-assistant.md) shows how Home Assistant turns the TV on, starts a video and turns the TV off. [cec.md](cec.md) describes the design for a USB-CEC adapter, which has no component yet.
