#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests the program htpc-power of the component power-key.

    tools/test-power-key.py

The tests need no root, no hardware and no python-evdev. They load the
program as a module, write a configuration file into a temporary folder,
and talk to a small HTTP server on the local machine. A fake evdev module
tests the watcher.
"""
import http.server
import importlib.machinery
import importlib.util
import json
import os
import tempfile
import threading
import types
import unittest
from pathlib import Path
from unittest import mock

PROGRAM = Path(__file__).resolve().parent.parent / "components/power-key/files/htpc-power"


def load_program():
    loader = importlib.machinery.SourceFileLoader("htpc_power", str(PROGRAM))
    spec = importlib.util.spec_from_loader("htpc_power", loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


class Server:
    """Answers each POST with a status and keeps the requests."""

    def __init__(self, status=200):
        self.requests = []
        outer = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                length = int(self.headers.get("Content-Length", 0))
                outer.requests.append((self.path, self.rfile.read(length)))
                self.send_response(status)
                self.end_headers()

            def log_message(self, *args):
                pass

        self.httpd = http.server.HTTPServer(("127.0.0.1", 0), Handler)
        self.port = self.httpd.server_address[1]
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()

    def close(self):
        self.httpd.shutdown()
        self.httpd.server_close()


class PowerTest(unittest.TestCase):
    def setUp(self):
        self.power = load_program()
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.marks = Path(self.dir.name) / "marks"
        self.marks.mkdir()
        self.config_path = Path(self.dir.name) / "htpc.conf"
        self.power.CONFIG = str(self.config_path)
        self.power.WAKE_SENDER = str(Path(self.dir.name) / "wake")
        self.write_wake_sender()

    def write_config(self, **values):
        lines = [f'{key}="{value}"' if " " in value else f"{key}={value}" for key, value in values.items()]
        self.config_path.write_text("\n".join(lines) + "\n")

    def write_wake_sender(self):
        sender = Path(self.power.WAKE_SENDER)
        sender.write_text(f"#!/bin/sh\ntouch {self.marks}/woke\n")
        sender.chmod(0o755)

    def mark(self, name):
        return f"touch {self.marks}/{name}"

    def marked(self, name):
        return (self.marks / name).exists()


class ConfigTest(PowerTest):
    def test_last_line_wins_and_one_pair_of_quotes_goes(self):
        self.config_path.write_text('A=1\n# B=3\nB=two\nB="x y"\nC=""\nD="say \'hi\'"\n')
        config = self.power.read_config()
        self.assertEqual(config["A"], "1")
        self.assertEqual(config["B"], "x y")
        self.assertEqual(config["C"], "")
        self.assertEqual(config["D"], "say 'hi'")

    def test_missing_file_is_an_empty_configuration(self):
        self.assertEqual(self.power.read_config(), {})


class StateTest(PowerTest):
    def test_state_command_exit_code(self):
        self.write_config(POWER_STATE_COMMAND="true")
        self.assertIs(self.power.is_on(self.power.read_config()), True)
        self.write_config(POWER_STATE_COMMAND="false")
        self.assertIs(self.power.is_on(self.power.read_config()), False)

    def test_hdmi_connector(self):
        for text, expected in (("connected\n", True), ("disconnected\n", False)):
            path = Path(self.dir.name) / "card0-HDMI-A-9"
            path.mkdir(exist_ok=True)
            (path / "status").write_text(text)
            with mock.patch.object(self.power.glob, "glob", return_value=[str(path / "status")]):
                self.assertIs(self.power.hdmi_connected("HDMI-A-9"), expected)

    def test_unknown_connector_and_bad_name(self):
        self.assertIsNone(self.power.hdmi_connected("HDMI-A-99"))
        self.assertIsNone(self.power.hdmi_connected("*"))

    def test_state_command_prints_and_exits(self):
        self.write_config(POWER_STATE_COMMAND="false")
        with mock.patch("builtins.print") as printed:
            self.assertEqual(self.power.print_state(), 1)
        printed.assert_called_with("off")


class StandaloneRouteTest(PowerTest):
    def test_toggle_turns_off_when_on(self):
        self.write_config(POWER_STATE_COMMAND="true", POWER_OFF_COMMAND=self.mark("off"),
                          POWER_ON_COMMAND=self.mark("on"), WAKE_ON_LAN_MACS="00:00:5e:00:53:01",
                          POWER_STOP_MEDIA="no")
        self.assertTrue(self.power.act("toggle"))
        self.assertTrue(self.marked("off"))
        self.assertFalse(self.marked("on") or self.marked("woke"))

    def test_toggle_turns_on_when_off_with_wake_and_command(self):
        self.write_config(POWER_STATE_COMMAND="false", POWER_OFF_COMMAND=self.mark("off"),
                          POWER_ON_COMMAND=self.mark("on"), WAKE_ON_LAN_MACS="00:00:5e:00:53:01")
        self.assertTrue(self.power.act("toggle"))
        self.assertTrue(self.marked("on") and self.marked("woke"))
        self.assertFalse(self.marked("off"))

    def test_unknown_state_turns_on(self):
        self.write_config(HDMI_OUTPUT="HDMI-A-99", POWER_ON_COMMAND=self.mark("on"),
                          POWER_OFF_COMMAND=self.mark("off"))
        self.assertTrue(self.power.act("toggle"))
        self.assertTrue(self.marked("on"))
        self.assertFalse(self.marked("off"))

    def test_wake_needs_a_mac_address(self):
        self.write_config(POWER_STATE_COMMAND="false")
        self.assertFalse(self.power.act("toggle"))
        self.assertFalse(self.marked("woke"))

    def test_off_without_command_fails(self):
        self.write_config(POWER_STATE_COMMAND="true")
        self.assertFalse(self.power.act("toggle"))

    def test_failed_command_fails(self):
        self.write_config(POWER_STATE_COMMAND="true", POWER_OFF_COMMAND="false", POWER_STOP_MEDIA="no")
        self.assertFalse(self.power.act("off"))

    def test_explicit_actions_ignore_the_state(self):
        self.write_config(POWER_STATE_COMMAND="true", POWER_ON_COMMAND=self.mark("on"))
        self.assertTrue(self.power.act("on"))
        self.assertTrue(self.marked("on"))

    def test_command_timeout_fails(self):
        self.power.COMMAND_TIMEOUT = 0.2
        self.write_config(POWER_OFF_COMMAND="sleep 5", POWER_STOP_MEDIA="no")
        self.assertFalse(self.power.act("off"))

    def test_off_stops_the_media_first(self):
        server = Server(204)
        self.addCleanup(server.close)
        self.write_config(POWER_OFF_COMMAND=self.mark("off"), CONTROL_API_PORT=str(server.port))
        self.assertTrue(self.power.act("off"))
        self.assertEqual(server.requests, [("/api/v1/home", b"")])
        self.assertTrue(self.marked("off"))

    def test_off_without_control_api_still_turns_off(self):
        server = Server()
        port = server.port
        server.close()
        self.write_config(POWER_OFF_COMMAND=self.mark("off"), CONTROL_API_PORT=str(port))
        self.assertTrue(self.power.act("off"))
        self.assertTrue(self.marked("off"))

    def test_stop_media_can_be_turned_off(self):
        server = Server(204)
        self.addCleanup(server.close)
        self.write_config(POWER_OFF_COMMAND=self.mark("off"), CONTROL_API_PORT=str(server.port),
                          POWER_STOP_MEDIA="no")
        self.assertTrue(self.power.act("off"))
        self.assertEqual(server.requests, [])


class WebhookRouteTest(PowerTest):
    def test_toggle_wakes_the_devices_before_the_webhook(self):
        server = Server()
        self.addCleanup(server.close)
        self.write_config(POWER_WEBHOOK_URL=f"http://127.0.0.1:{server.port}/hook",
                          WAKE_ON_LAN_MACS="00:00:5e:00:53:01")
        self.assertTrue(self.power.act("toggle"))
        self.assertTrue(self.marked("woke"))
        self.assertEqual(len(server.requests), 1)

    def test_an_explicit_off_sends_no_wake_packet(self):
        server = Server()
        self.addCleanup(server.close)
        self.write_config(POWER_WEBHOOK_URL=f"http://127.0.0.1:{server.port}/hook",
                          WAKE_ON_LAN_MACS="00:00:5e:00:53:01")
        self.assertTrue(self.power.act("off"))
        self.assertFalse(self.marked("woke"))

    def test_the_wake_packet_can_be_turned_off(self):
        server = Server()
        self.addCleanup(server.close)
        self.write_config(POWER_WEBHOOK_URL=f"http://127.0.0.1:{server.port}/hook",
                          WAKE_ON_LAN_MACS="00:00:5e:00:53:01", POWER_WEBHOOK_WAKE="no")
        self.assertTrue(self.power.act("toggle"))
        self.assertFalse(self.marked("woke"))

    def test_a_failed_wake_does_not_stop_the_webhook(self):
        server = Server()
        self.addCleanup(server.close)
        Path(self.power.WAKE_SENDER).write_text("#!/bin/sh\nexit 1\n")
        self.write_config(POWER_WEBHOOK_URL=f"http://127.0.0.1:{server.port}/hook",
                          WAKE_ON_LAN_MACS="00:00:5e:00:53:01")
        self.assertTrue(self.power.act("toggle"))
        self.assertEqual(len(server.requests), 1)

    def test_toggle_goes_to_home_assistant_and_runs_no_command(self):
        server = Server()
        self.addCleanup(server.close)
        self.write_config(POWER_WEBHOOK_URL=f"http://127.0.0.1:{server.port}/api/webhook/secret-id",
                          POWER_STATE_COMMAND=self.mark("state"), POWER_OFF_COMMAND=self.mark("off"),
                          POWER_ON_COMMAND=self.mark("on"), WAKE_ON_LAN_MACS="00:00:5e:00:53:01",
                          POWER_WEBHOOK_WAKE="no")
        self.assertTrue(self.power.act("toggle"))
        self.assertEqual(len(server.requests), 1)
        path, body = server.requests[0]
        self.assertEqual(path, "/api/webhook/secret-id")
        self.assertEqual(json.loads(body), {"action": "toggle"})
        for name in ("state", "off", "on", "woke"):
            self.assertFalse(self.marked(name), name)

    def test_explicit_action_is_sent(self):
        server = Server()
        self.addCleanup(server.close)
        self.write_config(POWER_WEBHOOK_URL=f"http://127.0.0.1:{server.port}/hook")
        self.assertTrue(self.power.act("off"))
        self.assertEqual(json.loads(server.requests[0][1]), {"action": "off"})

    def test_error_status_fails_without_the_address_in_the_log(self):
        server = Server(404)
        self.addCleanup(server.close)
        self.write_config(POWER_WEBHOOK_URL=f"http://127.0.0.1:{server.port}/api/webhook/secret-id")
        with mock.patch.object(self.power, "log") as log:
            self.assertFalse(self.power.act("toggle"))
        self.assertNotIn("secret-id", str(log.call_args_list))

    def test_unreachable_server_fails_without_the_address_in_the_log(self):
        server = Server()
        port = server.port
        server.close()
        self.write_config(POWER_WEBHOOK_URL=f"http://127.0.0.1:{port}/api/webhook/secret-id")
        with mock.patch.object(self.power, "log") as log:
            self.assertFalse(self.power.act("toggle"))
        self.assertNotIn("secret-id", str(log.call_args_list))

    def test_only_http_addresses(self):
        self.write_config(POWER_WEBHOOK_URL="file:///etc/passwd")
        self.assertFalse(self.power.act("toggle"))


class FakeEvdev:
    """Enough of python-evdev for the watcher."""

    ecodes = types.SimpleNamespace(
        EV_KEY=1, EV_SYN=0, EV_MSC=4, KEY_A=30,
        ecodes={"KEY_SLEEP": 142, "KEY_POWER": 116, "KEY_A": 30, "KEY_SEARCH": 217})

    class Device:
        def __init__(self, path, name="Remote", keys=(142,), grab_error=None):
            self.path, self.name, self.keys = path, name, keys
            self.closed = False
            self.grabbed = False
            self.grab_error = grab_error

        def capabilities(self):
            return {1: list(self.keys)}

        def grab(self):
            if self.grab_error:
                raise self.grab_error
            self.grabbed = True

        def close(self):
            self.closed = True

    class Forward:
        """A virtual device that records what the watcher writes to it."""

        def __init__(self, name):
            self.name = name
            self.written = []
            self.closed = False

        def write(self, type_, code, value):
            self.written.append((type_, code, value))

        def syn(self):
            self.written.append("syn")

        def close(self):
            self.closed = True

    class UInput:
        forwards = []

        @classmethod
        def from_device(cls, device, name):
            forward = FakeEvdev.Forward(name)
            cls.forwards.append(forward)
            return forward

    def __init__(self, devices):
        self.devices = {device.path: device for device in devices}
        self.UInput.forwards = []

    def list_devices(self):
        return list(self.devices)

    def InputDevice(self, path):
        return self.devices[path]


def key_event(code, value=1, type_=1):
    return types.SimpleNamespace(type=type_, code=code, value=value)


class WatcherCase(PowerTest):
    """A watcher with fake input devices. KEY_SLEEP is the power key."""

    codes = (142,)
    swallowed = ()

    def watcher(self, devices):
        return self.power.Watcher(FakeEvdev(devices), set(self.codes), "htpc", set(self.swallowed))


class WatcherTest(WatcherCase):

    def test_scan_keeps_remotes_and_skips_computer_buttons_and_other_devices(self):
        remote = FakeEvdev.Device("/dev/input/event1")
        button = FakeEvdev.Device("/dev/input/event2", name="Power Button", keys=(142, 116))
        keyboard = FakeEvdev.Device("/dev/input/event3", keys=(30,))
        watcher = self.watcher([remote, button, keyboard])
        watcher.scan()
        self.assertEqual(list(watcher.devices), ["/dev/input/event1"])
        self.assertTrue(button.closed and keyboard.closed and not remote.closed)

    def test_press_starts_one_job_as_the_user(self):
        watcher = self.watcher([])
        with mock.patch.object(self.power.subprocess, "Popen") as popen:
            popen.return_value.poll.return_value = None
            watcher.handle(key_event(142))
            watcher.handle(key_event(142))
        popen.assert_called_once_with(["runuser", "-u", "htpc", "--", self.power.PROGRAM, "toggle"])

    def test_release_repeat_and_other_keys_start_nothing(self):
        watcher = self.watcher([])
        with mock.patch.object(self.power.subprocess, "Popen") as popen:
            watcher.handle(key_event(142, value=0))
            watcher.handle(key_event(142, value=2))
            watcher.handle(key_event(30))
            watcher.handle(key_event(142, type_=2))
        popen.assert_not_called()

    def test_press_after_the_debounce_time_starts_a_new_job(self):
        watcher = self.watcher([])
        with mock.patch.object(self.power.subprocess, "Popen") as popen:
            popen.return_value.poll.return_value = 0
            watcher.handle(key_event(142))
            watcher.last_press -= self.power.DEBOUNCE + 0.1
            watcher.handle(key_event(142))
        self.assertEqual(popen.call_count, 2)

    def test_press_during_a_running_job_is_ignored(self):
        watcher = self.watcher([])
        with mock.patch.object(self.power.subprocess, "Popen") as popen:
            popen.return_value.poll.return_value = None
            watcher.handle(key_event(142))
            watcher.last_press -= self.power.DEBOUNCE + 0.1
            watcher.handle(key_event(142))
        popen.assert_called_once()


class SwallowTest(WatcherCase):
    """KEY_SEARCH is a power key that the system does not ignore."""

    SEARCH = 217
    codes = (142, SEARCH)
    swallowed = (SEARCH,)

    def test_a_remote_with_the_key_is_taken(self):
        remote = FakeEvdev.Device(
            "/dev/input/event1", name="Remote Consumer Control", keys=(self.SEARCH, 115))
        watcher = self.watcher([remote])
        watcher.scan()
        self.assertTrue(remote.grabbed)
        self.assertEqual([f.name for f in watcher.evdev.UInput.forwards],
                         ["Remote Consumer Control (x86-linux-htpc power key)"])

    def test_a_keyboard_stays_free(self):
        keyboard = FakeEvdev.Device("/dev/input/event1", name="Keyboard", keys=(30, self.SEARCH))
        watcher = self.watcher([keyboard])
        watcher.scan()
        self.assertFalse(keyboard.grabbed)
        self.assertIn("/dev/input/event1", watcher.devices)

    def test_the_air_mouse_device_is_taken_although_it_has_the_keys_of_a_keyboard(self):
        mouse = FakeEvdev.Device("/dev/input/event1", name="x86-linux-htpc air mouse", keys=(30, self.SEARCH))
        watcher = self.watcher([mouse])
        watcher.scan()
        self.assertTrue(mouse.grabbed)

    def test_system_ignored_keys_take_no_device(self):
        remote = FakeEvdev.Device("/dev/input/event1", keys=(142,))
        watcher = self.watcher([remote])
        watcher.scan()
        self.assertFalse(remote.grabbed)
        self.assertIn("/dev/input/event1", watcher.devices)

    def test_a_device_that_cannot_be_taken_is_only_watched(self):
        remote = FakeEvdev.Device("/dev/input/event1", keys=(self.SEARCH,), grab_error=OSError("busy"))
        watcher = self.watcher([remote])
        with mock.patch.object(self.power, "log"):
            watcher.scan()
        self.assertEqual(watcher.forwards, {})
        self.assertIn("/dev/input/event1", watcher.devices)

    def test_a_failed_grab_closes_the_virtual_device(self):
        remote = FakeEvdev.Device("/dev/input/event1", keys=(self.SEARCH,), grab_error=OSError("busy"))
        watcher = self.watcher([remote])
        with mock.patch.object(self.power, "log"):
            watcher.scan()
        self.assertTrue(watcher.evdev.UInput.forwards[0].closed)

    def test_no_virtual_device_means_no_grab(self):
        remote = FakeEvdev.Device("/dev/input/event1", keys=(self.SEARCH,))
        watcher = self.watcher([remote])
        with mock.patch.object(self.power, "log"), \
                mock.patch.object(FakeEvdev.UInput, "from_device", side_effect=OSError("no uinput")):
            watcher.scan()
        self.assertFalse(remote.grabbed)
        self.assertIn("/dev/input/event1", watcher.devices)

    def test_the_virtual_device_is_not_scanned_again(self):
        remote = FakeEvdev.Device("/dev/input/event1", keys=(self.SEARCH,))
        watcher = self.watcher([remote])
        watcher.scan()
        clone = FakeEvdev.Device("/dev/input/event2", name=remote.name + self.power.FORWARD_SUFFIX,
                                 keys=(self.SEARCH,))
        watcher.evdev.devices[clone.path] = clone
        watcher.scan()
        self.assertNotIn(clone.path, watcher.devices)

    def test_the_key_starts_the_job_and_reaches_no_program(self):
        remote = FakeEvdev.Device("/dev/input/event1", keys=(self.SEARCH, 115))
        watcher = self.watcher([remote])
        watcher.scan()
        forward = watcher.forwards[remote.path]
        with mock.patch.object(self.power.subprocess, "Popen") as popen:
            watcher.handle(key_event(self.SEARCH, 1), remote.path)
            watcher.handle(key_event(self.SEARCH, 0), remote.path)
            watcher.handle(types.SimpleNamespace(type=4, code=4, value=0xc0221), remote.path)
        popen.assert_called_once()
        self.assertEqual(forward.written, [(4, 4, 0xc0221)])

    def test_other_events_pass_on(self):
        remote = FakeEvdev.Device("/dev/input/event1", keys=(self.SEARCH, 115))
        watcher = self.watcher([remote])
        watcher.scan()
        forward = watcher.forwards[remote.path]
        with mock.patch.object(self.power.subprocess, "Popen") as popen:
            watcher.handle(key_event(115, 1), remote.path)
            watcher.handle(key_event(115, 0), remote.path)
            watcher.handle(types.SimpleNamespace(type=0, code=0, value=0), remote.path)
        popen.assert_not_called()
        self.assertEqual(forward.written, [(1, 115, 1), (1, 115, 0), "syn"])

    def test_unplugging_closes_the_virtual_device(self):
        remote = FakeEvdev.Device("/dev/input/event1", keys=(self.SEARCH,))
        watcher = self.watcher([remote])
        watcher.scan()
        forward = watcher.forwards[remote.path]
        watcher.drop(remote)
        self.assertTrue(forward.closed and remote.closed)
        self.assertEqual((watcher.devices, watcher.forwards), ({}, {}))


class ProgramTest(PowerTest):
    def run_main(self, *args):
        with mock.patch.object(self.power.sys, "argv", ["htpc-power", *args]):
            with self.assertRaises(SystemExit) as raised:
                self.power.main()
        return raised.exception.code

    def test_exit_codes(self):
        self.write_config(POWER_STATE_COMMAND="true", POWER_OFF_COMMAND="true", POWER_STOP_MEDIA="no")
        self.assertEqual(self.run_main("off"), 0)
        self.write_config(POWER_STATE_COMMAND="true", POWER_OFF_COMMAND="false", POWER_STOP_MEDIA="no")
        self.assertEqual(self.run_main("toggle"), 1)

    def test_usage(self):
        self.assertIn("Usage", self.run_main("nonsense"))
        self.assertIn("Usage", self.run_main())


if __name__ == "__main__":
    os.environ.pop("HTPC_CONFIG", None)
    unittest.main()
