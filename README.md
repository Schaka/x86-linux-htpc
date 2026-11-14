# x86-linux-htpc

This project turns a normal x86 computer with Linux into a home theater PC (HTPC). The HTPC plays video from a media server and from streaming apps on a TV, with sound through an AV receiver.

The project gives you the source code, the scripts and the instructions to build the same system again on other hardware. It does not replace the operating system with a media-center distribution. You keep a normal Linux desktop under the HTPC interface.

## Goals

The finished system does these things out of the box:

1. It plays Dolby Vision, HDR10, HDR10+, HLG and SDR video with the correct output signal to the TV.
2. It sends all audio formats unchanged to the AV receiver: Dolby Digital, Dolby Digital Plus, Dolby TrueHD, Dolby Atmos, DTS, DTS-HD High Resolution, DTS-HD Master Audio and DTS:X.
3. It plays media from a Jellyfin server with `jellyfin-mpv-shim`. Kodi is an optional second player.
4. It uses KDE Plasma Bigscreen as the TV interface.
5. It runs Android TV apps for YouTube and Twitch in Waydroid.
6. It turns the TV on over the network, and it takes the keys of an air mouse, a USB remote or a keyboard. Home Assistant can switch the TV and start videos.

The project supports Fedora officially. The scripts keep the distribution-specific parts small, so that other distributions are easy to add.

## Where to start

| You want to | Read |
|---|---|
| Know what works today | [docs/status.md](docs/status.md) |
| Understand how the parts fit together | [docs/architecture.md](docs/architecture.md) |
| Find out whether your hardware is suitable | [docs/hardware.md](docs/hardware.md) |
| Install the HTPC on a fresh Fedora | [docs/install.md](docs/install.md), or let an AI agent use the skill `setup-htpc` |
| Install or check parts of the HTPC | `./install.sh --help` |
| Work on the project as an AI agent | [AGENTS.md](AGENTS.md) |

## Documentation

| File | Content |
|---|---|
| [docs/status.md](docs/status.md) | What works in this version, and on which hardware |
| [docs/install.md](docs/install.md) | The setup on a fresh Fedora, its questions and the builds for Dolby Vision and Kodi |
| [docs/architecture.md](docs/architecture.md) | The components, who owns the screen, and the flow from a key press to the picture |
| [docs/hardware.md](docs/hardware.md) | Requirements for the GPU, the HDMI port, the audio driver, the TV and the AV receiver, and the known hardware |
| [docs/video.md](docs/video.md) | How each HDR format reaches the TV, and which player handles it |
| [docs/audio.md](docs/audio.md) | Audio passthrough, the HDMI audio driver choices, PipeWire and the decode fallback |
| [docs/kernel.md](docs/kernel.md) | Why the project needs a patched kernel, and how the patch set is built |
| [docs/players.md](docs/players.md) | `jellyfin-mpv-shim`, mpv and Kodi with the CB1 engine |
| [docs/cb1-integration.md](docs/cb1-integration.md) | The calls of the CB1 engine and their places in mpv |
| [docs/desktop.md](docs/desktop.md) | The Plasma Bigscreen session, start-up, display modes and power |
| [docs/android-apps.md](docs/android-apps.md) | Waydroid and the Android TV apps for YouTube and Twitch |
| [docs/tv-control.md](docs/tv-control.md) | The ways to turn the TV on and off, select the input and take remote keys, with and without CEC |
| [docs/control-api.md](docs/control-api.md) | The REST API and the DLNA renderer that start media on the HTPC and control it |
| [docs/api.md](docs/api.md) | The paths of the REST API with fields and examples, and the link to the OpenAPI document of the newest release |
| [docs/home-assistant.md](docs/home-assistant.md) | Home Assistant scripts that turn the TV on and off and start a video on the HTPC |
| [docs/cec.md](docs/cec.md) | HDMI-CEC with a USB adapter, the remote keys and the power control |
| [docs/distributions.md](docs/distributions.md) | Fedora support, and the rules that keep the scripts portable |
| [docs/testing.md](docs/testing.md) | How to check each format on the HTPC and on the TV, and how to report a problem |
| [docs/upstream.md](docs/upstream.md) | The external projects that this project uses, and their licenses |

## License

The own files of this project are under the GNU Affero General Public License v3.0 only. See [LICENSE](LICENSE).

Patches to the Linux kernel are under GPL-2.0-only, because the kernel license does not allow another license for them. See [LICENSES/GPL-2.0-only.txt](LICENSES/GPL-2.0-only.txt). Files from other projects keep their own licenses. [docs/upstream.md](docs/upstream.md) lists them.

The skill in `.agents/skills/simple-english/` is under the MIT license. Its license file is in the same folder.
