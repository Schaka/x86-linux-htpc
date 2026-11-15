#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests the casting of a running HTPC from the network, as a phone does.

  tools/test-casting.py HOST [--port 8780] [--ssh USER@HOST] [--video] [--launch]

HOST is the address of the HTPC. The script sends the searches and requests
of a DLNA app, of the YouTube app (DIAL) and of an AirPlay sender, and
checks the answers. Each check prints PASS or FAIL. The exit code is 0 when
all checks pass.

The music tests play a silent file. They need ffmpeg to make it.

  --ssh TARGET  also reads the active VT and the mpv processes of the HTPC
                over SSH, to check that music leaves the TV on Plasma
  --video       also casts a silent test video. It wakes the TV and takes the
                screen for a few seconds.
  --launch      also starts YouTube through DIAL, if VacuumTube is closed. It
                wakes the TV. With --ssh, the script closes VacuumTube again.
"""
import argparse
import http.server as http_server
import json
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path
from xml.sax.saxutils import escape

DIAL = "urn:dial-multiscreen-org:service:dial:1"
RENDERER = "urn:schemas-upnp-org:device:MediaRenderer:1"
AVT = "urn:schemas-upnp-org:service:AVTransport:1"
ORIGIN = "package:com.google.android.youtube"

failed = False


def check(description, ok, detail=""):
    global failed
    print(("PASS " if ok else "FAIL ") + description + ("" if ok or not detail else f" ({detail})"))
    if not ok:
        failed = True
    return ok


def skip(description, reason):
    print(f"SKIP {description} ({reason})")


def wait_for(condition, seconds, pause=0.5):
    end = time.time() + seconds
    while time.time() < end:
        value = condition()
        if value:
            return value
        time.sleep(pause)
    return None


# Network helpers

def http(method, url, body=None, headers=None, timeout=15):
    """Returns (status, headers, text). A connection error gives status 0."""
    data = body.encode() if isinstance(body, str) else body
    request = urllib.request.Request(url, data=data, method=method, headers=headers or {})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as answer:
            return answer.status, answer.headers, answer.read().decode(errors="replace")
    except urllib.error.HTTPError as error:
        return error.code, error.headers, error.read().decode(errors="replace")
    except (urllib.error.URLError, OSError, TimeoutError):
        return 0, {}, ""


def ssdp_search(target, host, seconds=3):
    """The answers of the host to a search, as a list of header dictionaries"""
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(0.5)
    message = (
        "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\n"
        f"MX: 2\r\nST: {target}\r\nUSER-AGENT: htpc-casting-test/1.0\r\n\r\n"
    )
    answers = []
    end = time.time() + seconds
    sock.sendto(message.encode(), ("239.255.255.250", 1900))
    while time.time() < end:
        try:
            data, peer = sock.recvfrom(4096)
        except socket.timeout:
            continue
        if peer[0] != host:
            continue
        headers = {}
        for line in data.decode(errors="replace").split("\r\n")[1:]:
            if ":" in line:
                key, value = line.split(":", 1)
                headers[key.strip().upper()] = value.strip()
        answers.append(headers)
    return answers


def local_address_towards(host):
    probe = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    probe.connect((host, 9))
    return probe.getsockname()[0]


def tag(xml, name):
    found = re.search(rf"<(?:\w+:)?{name}>([^<]*)</", xml)
    return found.group(1) if found else None


# The HTPC

class Htpc:
    def __init__(self, host, port, ssh):
        self.host, self.port, self.ssh = host, port, ssh
        self.api = f"http://{host}:{port}"

    def status(self):
        code, _, text = http("GET", f"{self.api}/api/v1/status")
        return json.loads(text) if code == 200 else {}

    def soap(self, action, arguments=""):
        body = (
            '<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" '
            's:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body>'
            f'<u:{action} xmlns:u="{AVT}"><InstanceID>0</InstanceID>{arguments}</u:{action}>'
            "</s:Body></s:Envelope>"
        )
        return http(
            "POST",
            f"{self.api}/dlna/AVTransport/control",
            body,
            {"Content-Type": 'text/xml; charset="utf-8"', "SOAPACTION": f'"{AVT}#{action}"'},
        )

    def transport_state(self):
        _, _, text = self.soap("GetTransportInfo")
        return tag(text, "CurrentTransportState")

    def cast(self, address, metadata=""):
        arguments = (
            f"<CurrentURI>{escape(address)}</CurrentURI>"
            f"<CurrentURIMetaData>{escape(metadata)}</CurrentURIMetaData>"
        )
        set_code, _, _ = self.soap("SetAVTransportURI", arguments)
        play_code, _, _ = self.soap("Play", "<Speed>1</Speed>")
        return set_code == 200 and play_code == 200

    def stop_and_wait(self):
        self.soap("Stop")
        return wait_for(lambda: self.status().get("state") == "idle", 15)

    def over_ssh(self, command):
        if not self.ssh:
            return None
        result = subprocess.run(
            ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=5", self.ssh, command],
            capture_output=True, text=True, timeout=30,
        )
        return result.stdout.strip()


def didl(item_class, title, address):
    return (
        '<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" '
        'xmlns:dc="http://purl.org/dc/elements/1.1/" '
        'xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">'
        f'<item id="1" parentID="0" restricted="1"><dc:title>{escape(title)}</dc:title>'
        f"<upnp:class>{item_class}</upnp:class>"
        f'<res protocolInfo="http-get:*:*:*">{escape(address)}</res></item></DIDL-Lite>'
    )


# The media server of the test

class Media:
    """Serves the test files from a folder on a free port."""

    def __init__(self, folder, host):
        handler = lambda *a, **k: http_server.SimpleHTTPRequestHandler(*a, directory=str(folder), **k)
        handler.log_message = lambda *a: None
        self.server = http_server.ThreadingHTTPServer((local_address_towards(host), 0), handler)
        self.base = f"http://{self.server.server_address[0]}:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self):
        self.server.shutdown()


def make_media(folder):
    """A silent MP3, and a silent test video. The video has its index at the
    start, because the small test server cannot send byte ranges."""
    if not shutil.which("ffmpeg"):
        return False
    quiet = ["ffmpeg", "-loglevel", "error", "-y"]
    subprocess.run(quiet + ["-f", "lavfi", "-i", "anullsrc=r=44100:cl=stereo", "-t", "20",
                            "-c:a", "libmp3lame", "-b:a", "32k", str(folder / "silence.mp3")], check=True)
    shutil.copy(folder / "silence.mp3", folder / "song")
    subprocess.run(quiet + ["-f", "lavfi", "-i", "testsrc=size=1280x720:rate=25",
                            "-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo", "-t", "15",
                            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac",
                            "-movflags", "+faststart", "-shortest", str(folder / "pattern.mp4")], check=True)
    return True


# The checks

def test_api(htpc):
    code, _, text = http("GET", f"{htpc.api}/health")
    check("The API answers /health", code == 200 and '"ok"' in text, f"{code}")
    status = htpc.status()
    check("The status has the fields of the API",
          {"source", "state", "running"} <= set(status), str(status))
    return status


def test_dlna_discovery(htpc):
    answers = ssdp_search(RENDERER, htpc.host)
    check("The renderer answers the SSDP search of a DLNA app", len(answers) >= 1)
    location = answers[0].get("LOCATION") if answers else None
    code, _, text = http("GET", location) if location else (0, None, "")
    check("The description of the renderer has the media renderer and its services",
          code == 200 and "MediaRenderer" in text and "AVTransport" in text
          and "RenderingControl" in text and "ConnectionManager" in text, f"{code}")
    code, _, text = http("POST", f"{htpc.api}/dlna/ConnectionManager/control",
                         '<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">'
                         '<s:Body><u:GetProtocolInfo xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:1"/>'
                         "</s:Body></s:Envelope>",
                         {"Content-Type": 'text/xml; charset="utf-8"',
                          "SOAPACTION": '"urn:schemas-upnp-org:service:ConnectionManager:1#GetProtocolInfo"'})
    check("The renderer lists audio formats as sink protocols",
          code == 200 and "audio/mpeg" in text and "audio/flac" in text, f"{code}")


def music_flow(htpc, description, address, metadata, ssh_check=True):
    """Casts music and walks through pause, seek, play and stop."""
    if not check(f"{description}: the cast is accepted", htpc.cast(address, metadata)):
        return
    playing = wait_for(lambda: htpc.status() if htpc.status().get("state") == "playing" else None, 20)
    if not check(f"{description}: the status shows the music as playing", bool(playing)):
        htpc.stop_and_wait()
        return
    check(f"{description}: the source is the url player", playing["source"] == "url", playing["source"])
    check(f"{description}: the duration is the one of the file",
          abs((playing.get("duration") or 0) - 20) < 1.5, str(playing.get("duration")))
    first = playing.get("position") or 0
    time.sleep(2)
    second = htpc.status().get("position") or 0
    check(f"{description}: the position moves", second > first, f"{first} to {second}")
    check(f"{description}: GetTransportInfo says PLAYING", htpc.transport_state() == "PLAYING")
    if ssh_check and htpc.ssh:
        vt = htpc.over_ssh("cat /sys/class/tty/tty0/active")
        mpv = htpc.over_ssh("pgrep -af '[m]pv .*input-ipc-server' | head -1")
        check(f"{description}: the TV stays on Plasma", vt == "tty1", vt)
        check(f"{description}: mpv has no picture", "--no-video" in (mpv or ""), mpv)
    htpc.soap("Pause")
    paused = wait_for(lambda: htpc.status().get("state") == "paused", 8)
    check(f"{description}: Pause pauses", bool(paused))
    check(f"{description}: GetTransportInfo says PAUSED_PLAYBACK", htpc.transport_state() == "PAUSED_PLAYBACK")
    htpc.soap("Seek", "<Unit>REL_TIME</Unit><Target>00:00:12</Target>")
    sought = wait_for(lambda: (htpc.status().get("position") or 0) >= 11.5, 8)
    check(f"{description}: Seek moves to 12 seconds", bool(sought), str(htpc.status().get("position")))
    htpc.soap("Play", "<Speed>1</Speed>")
    resumed = wait_for(lambda: htpc.status().get("state") == "playing", 8)
    check(f"{description}: Play continues", bool(resumed))
    check(f"{description}: Stop ends the music", bool(htpc.stop_and_wait()))
    check(f"{description}: GetTransportInfo says STOPPED or NO_MEDIA_PRESENT",
          htpc.transport_state() in ("STOPPED", "NO_MEDIA_PRESENT"))


def test_dlna_music(htpc, media):
    music_flow(htpc, "Music with an mp3 address", f"{media.base}/silence.mp3",
               didl("object.item.audioItem.musicTrack", "Test song", f"{media.base}/silence.mp3"))
    music_flow(htpc, "Music without a file extension, with the audio class", f"{media.base}/song",
               didl("object.item.audioItem.musicTrack", "Test song", f"{media.base}/song"), ssh_check=False)
    code, _, _ = http("POST", f"{htpc.api}/api/v1/play/url",
                      json.dumps({"url": f"{media.base}/song", "audio": True, "title": "Rest song"}),
                      {"Content-Type": "application/json"})
    check("The REST API accepts an audio address with the audio flag", code == 202, f"{code}")
    started = wait_for(lambda: htpc.status().get("state") == "playing", 20)
    check("The REST API plays it with its title", bool(started) and htpc.status().get("title") == "Rest song",
          str(htpc.status().get("title")))
    htpc.stop_and_wait()
    code, _, _ = http("POST", f"{htpc.api}/api/v1/play/url", json.dumps({"url": "file:///etc/passwd"}),
                      {"Content-Type": "application/json"})
    check("The REST API refuses a file address", code == 400, f"{code}")


def test_dlna_video(htpc, media):
    check("The cast of a video is accepted",
          htpc.cast(f"{media.base}/pattern.mp4", didl("object.item.videoItem", "Pattern", f"{media.base}/pattern.mp4")))
    playing = wait_for(lambda: htpc.status() if htpc.status().get("state") == "playing" else None, 40)
    check("The video plays", bool(playing))
    if htpc.ssh and playing:
        mpv = htpc.over_ssh("pgrep -af '[m]pv .*input-ipc-server' | head -1")
        check("The video mpv has a picture", mpv and "--no-video" not in mpv and "--fullscreen" in mpv, mpv)
    check("Stop ends the video", bool(htpc.stop_and_wait()))
    time.sleep(3)
    if htpc.ssh:
        check("The screen goes back to Plasma", htpc.over_ssh("cat /sys/class/tty/tty0/active") == "tty1")


def dial_answers(htpc):
    return ssdp_search(DIAL, htpc.host)


def test_dial(htpc):
    answers = dial_answers(htpc)
    check("The HTPC answers the DIAL search of the YouTube app once", len(answers) == 1, f"{len(answers)} answers")
    if not answers:
        return None
    location = answers[0].get("LOCATION", "")
    code, headers, text = http("GET", location, headers={"Origin": ORIGIN})
    check("The DIAL description has the device type and a name",
          code == 200 and "dial-multiscreen-org:device:dial:1" in text and bool(tag(text, "friendlyName")), f"{code}")
    udn = tag(text, "UDN")
    usn = answers[0].get("USN", "")
    check("The USN of the search answer has the UDN of the description", bool(udn) and usn.startswith(udn), usn)
    application_url = headers.get("Application-URL", "") if headers else ""
    check("The description has the Application-URL header", application_url.startswith("http://")
          and application_url.endswith("/apps/"), application_url)
    for name in ("YouTube", "youtube"):
        code, _, state = http("GET", application_url + name, headers={"Origin": ORIGIN})
        check(f"The app resource /{name} answers with a state", code == 200 and tag(state, "state") in ("running", "stopped"),
              f"{code}")
    code, _, _ = http("GET", application_url + "Netflix", headers={"Origin": ORIGIN})
    check("An app that the HTPC does not have is 404", code == 404, f"{code}")
    return application_url, udn, tag(state, "state")


def test_launch(htpc, dial):
    if not dial:
        return skip("The DIAL launch", "no DIAL device")
    application_url, udn, state = dial
    if state != "stopped":
        return skip("The DIAL launch", "VacuumTube runs already")
    started = time.time()
    code, headers, _ = http("POST", application_url + "YouTube", "pairingCode=00000000-0000-0000-0000-000000000000&theme=cl",
                            {"Content-Type": "text/plain; charset=utf-8", "Origin": ORIGIN})
    check("The launch answers 201 at once", code == 201 and time.time() - started < 3.5, f"{code}, {time.time() - started:.1f} s")
    check("The launch answer has the address of the instance",
          bool(headers) and headers.get("Location", "").endswith("/apps/YouTube/run"))
    state = wait_for(lambda: (lambda c, h, t: t if c == 200 and tag(t, "state") == "running" else None)(
        *http("GET", application_url + "YouTube", headers={"Origin": ORIGIN})), 60)
    check("The state becomes running without a stopped answer in between", bool(state))
    time.sleep(3)
    answers = dial_answers(htpc)
    check("While VacuumTube runs, the phone sees one device", len(answers) == 1, f"{len(answers)} answers")
    if answers:
        code, _, text = http("GET", answers[0].get("LOCATION", ""))
        check("VacuumTube has the UDN that the HTPC used before", tag(text, "UDN") == udn,
              f"{tag(text, 'UDN')} and {udn}")
    if htpc.ssh:
        htpc.over_ssh("flatpak kill rocks.shy.VacuumTube")
        time.sleep(4)
        answers = dial_answers(htpc)
        check("After VacuumTube closes, the phone sees one device", len(answers) == 1, f"{len(answers)} answers")
        if answers:
            code, _, text = http("GET", answers[0].get("LOCATION", ""))
            check("The HTPC keeps the UDN of VacuumTube", tag(text, "UDN") == udn)
    else:
        print("NOTE VacuumTube is open now. Close it on the HTPC.")


def test_airplay(htpc):
    if not shutil.which("avahi-browse"):
        return skip("The AirPlay receiver", "avahi-browse is missing")
    output = subprocess.run(["avahi-browse", "-rtp", "_airplay._tcp"], capture_output=True, text=True, timeout=30).stdout
    mine = [line.split(";") for line in output.splitlines() if line.startswith("=") and htpc.host in line.split(";")[7:8]]
    if not mine:
        return skip("The AirPlay receiver", "the HTPC does not run it")
    ok = check("The AirPlay receiver is found with mDNS", True)
    port = int(mine[0][8])
    try:
        with socket.create_connection((htpc.host, port), timeout=5):
            reachable = True
    except OSError:
        reachable = False
    check("The AirPlay port accepts connections", reachable, f"port {port}")
    # AirPlay speaks RTSP on this port, not HTTP.
    answers = {}
    for request in ("OPTIONS * RTSP/1.0", "GET /info RTSP/1.0"):
        try:
            with socket.create_connection((htpc.host, port), timeout=5) as connection:
                connection.settimeout(5)
                connection.sendall(f"{request}\r\nCSeq: 1\r\nUser-Agent: AirPlay/550.10\r\n\r\n".encode())
                answers[request] = connection.recv(200).decode(errors="replace")
        except OSError as error:
            answers[request] = str(error)
    check("The AirPlay receiver answers an RTSP OPTIONS request",
          answers["OPTIONS * RTSP/1.0"].startswith("RTSP/1.0 200"), answers["OPTIONS * RTSP/1.0"][:40])
    check("The AirPlay receiver answers /info", answers["GET /info RTSP/1.0"].startswith("RTSP/1.0 200"),
          answers["GET /info RTSP/1.0"][:40])
    return ok


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("host")
    parser.add_argument("--port", type=int, default=8780)
    parser.add_argument("--ssh")
    parser.add_argument("--video", action="store_true")
    parser.add_argument("--launch", action="store_true")
    args = parser.parse_args()
    htpc = Htpc(args.host, args.port, args.ssh)

    status = test_api(htpc)
    if status.get("state") not in (None, "idle"):
        print("NOTE Something plays on the HTPC. The music tests stop it.")
    test_dlna_discovery(htpc)
    folder = Path(tempfile.mkdtemp(prefix="htpc-casting-test-"))
    media = None
    try:
        if make_media(folder):
            media = Media(folder, args.host)
            test_dlna_music(htpc, media)
            if args.video:
                test_dlna_video(htpc, media)
            else:
                skip("The video cast", "use --video")
        else:
            skip("The DLNA music and video casts", "ffmpeg is missing")
        dial = test_dial(htpc)
        if args.launch:
            test_launch(htpc, dial)
        else:
            skip("The DIAL launch", "use --launch")
        test_airplay(htpc)
    finally:
        htpc.soap("Stop")
        if media:
            media.close()
        shutil.rmtree(folder, ignore_errors=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
