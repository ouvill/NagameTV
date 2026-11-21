"""Validate the Raspberry Pi boot media payload without starting services."""
from contextlib import redirect_stderr
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import call, Mock, patch
from xml.etree import ElementTree

try:
    import yaml
except ModuleNotFoundError:
    yaml = None


PAYLOAD = Path(__file__).resolve().parents[2] / "packaging/pi-appliance"
NETWORK_CONFIGS = ("network-config", "network-config.wifi")
RECOVERY_SERVICE = "nagametv-hdmi-audio.service"
RECOVERY_SCRIPT = "hdmi-audio.py"
WIREPLUMBER_CONF = "50-nagametv-hdmi-only.conf"
PULSE_CONF = "50-nagametv-hdmi-stream.conf"

spec = importlib.util.spec_from_file_location("hdmi_audio", PAYLOAD / RECOVERY_SCRIPT)
hdmi_audio = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hdmi_audio)


class PiApplianceTests(unittest.TestCase):
    @unittest.skipUnless(yaml is not None, "PyYAML is required to parse appliance YAML")
    def test_seed_configures_key_login_and_runs_the_boot_payload(self):
        source = (PAYLOAD / "user-data").read_text()
        self.assertTrue(source.startswith("#cloud-config\n"))
        seed = yaml.safe_load(source)
        admin, kiosk = seed["users"]
        self.assertIs(seed["ssh_pwauth"], False)
        self.assertIs(seed["disable_root"], True)
        self.assertEqual(admin["ssh_authorized_keys"], ["REPLACE_WITH_YOUR_SSH_PUBLIC_KEY"])
        self.assertTrue(admin["lock_passwd"])
        self.assertTrue(kiosk["lock_passwd"])
        self.assertIn("video", kiosk["groups"])
        # WirePlumber starts before the kiosk session gets the seat ACLs.
        self.assertIn("audio", kiosk["groups"])
        self.assertNotIn("sudo", kiosk)
        files = {item["path"]: item["content"] for item in seed["write_files"]}
        self.assertIn("AuthenticationMethods publickey\n", files["/etc/ssh/sshd_config.d/00-nagametv-appliance.conf"])
        self.assertIn("session required pam_systemd.so\n", files["/etc/pam.d/nagametv-kiosk"])
        for executable, path in seed["runcmd"]:
            self.assertEqual(executable, "/bin/bash")
            self.assertEqual(Path(path).parent, Path("/boot/firmware"))
            self.assertTrue((PAYLOAD / Path(path).name).is_file())
        unit = (PAYLOAD / "nagametv-kiosk.service").read_text()
        self.assertIn(f"User={kiosk['name']}\n", unit)
        self.assertIn("PAMName=nagametv-kiosk\n", unit)

    def test_kiosk_selects_greedyl_at_one_field_per_frame(self):
        unit = (PAYLOAD / "nagametv-kiosk.service").read_text().splitlines()
        self.assertIn("Environment=NAGAMETV_DEINTERLACE=greedyl", unit)
        self.assertIn("Environment=NAGAMETV_DEINTERLACE_FIELDS=one", unit)

    @unittest.skipUnless(yaml is not None, "PyYAML is required to parse appliance YAML")
    def test_network_examples_require_the_selected_interface(self):
        wired, wifi = (yaml.safe_load((PAYLOAD / name).read_text()) for name in NETWORK_CONFIGS)
        self.assertEqual(wired, {"version": 2, "ethernets": {"eth0": {"dhcp4": True, "optional": False}}})
        self.assertIs(wifi["ethernets"]["eth0"]["optional"], True)
        wireless = wifi["wifis"]["wlan0"]
        self.assertIs(wireless["dhcp4"], True)
        self.assertIs(wireless["optional"], False)
        self.assertEqual(wireless["access-points"], {
            "REPLACE_WITH_WIFI_SSID": {"password": "REPLACE_WITH_WIFI_PASSWORD"}
        })

    @unittest.skipUnless(shutil.which("cloud-init"), "cloud-init is required for schema validation")
    def test_cloud_init_schemas(self):
        for name in ("user-data", *NETWORK_CONFIGS):
            with self.subTest(name=name):
                kind = "cloud-config" if name == "user-data" else "network-config"
                subprocess.run(["cloud-init", "schema", "--schema-type", kind,
                                "--config-file", str(PAYLOAD / name)], check=True)

    @unittest.skipUnless(shutil.which("systemd-analyze"), "systemd-analyze is required for unit verification")
    def test_systemd_unit_verifies_in_an_isolated_root(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            units = root / "usr/lib/systemd/system"
            units.mkdir(parents=True)
            unit_names = ("nagametv-kiosk.service", RECOVERY_SERVICE)
            for name in unit_names:
                shutil.copyfile(PAYLOAD / name, units / name)
            for name in ("sysinit", "basic", "shutdown", "multi-user"):
                (units / f"{name}.target").write_text("[Unit]\nDefaultDependencies=no\n")
            # verify checks executable paths in the offline root. It never
            # starts the compositor, changes the VT, or connects to host systemd.
            for name in ("labwc", "chvt", "python3"):
                command = root / "usr/bin" / name
                command.parent.mkdir(parents=True, exist_ok=True)
                command.touch(mode=0o755)
            result = subprocess.run(["systemd-analyze", f"--root={root}", "--man=no",
                                     "--generators=no", "--recursive-errors=yes", "verify", *unit_names],
                                    env=dict(os.environ, SYSTEMD_LOG_LEVEL="warning"),
                                    text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(result.stderr, "")

    def test_labwc_config_leaves_fullscreen_to_the_client(self):
        root = ElementTree.parse(PAYLOAD / "labwc-rc.xml").getroot()
        self.assertEqual(root.tag, "labwc_config")
        rules = root.findall("windowRules/windowRule")
        self.assertEqual([rule.attrib for rule in rules],
                         [{"identifier": "*", "serverDecoration": "no"}])
        self.assertEqual(rules[0].findall("action"), [])
        provision = (PAYLOAD / "provision-nagametv.sh").read_text()
        self.assertIn("labwc-rc.xml", provision)
        self.assertIn("/usr/local/lib/nagametv-appliance/labwc/rc.xml", provision)

    def test_optional_connection_example_is_imported_as_the_kiosk_user_before_start(self):
        example = PAYLOAD / "nagametv-connection.toml.example"
        seed = tomllib.loads(example.read_text())
        self.assertEqual(seed, {"server": "http://192.168.1.100:40772"})
        self.assertIn('# epgstation_server = "http://192.168.1.100:8888"', example.read_text())
        provision = (PAYLOAD / "provision-nagametv.sh").read_text()
        self.assertIn('connection="$boot/nagametv-connection.toml"', provision)
        self.assertIn('if [[ -e $connection ]]; then', provision)
        self.assertIn('runuser -u nagametv', provision)
        self.assertIn('--import-connection < "$connection"; then', provision)
        self.assertLess(provision.index('--import-connection'), provision.index('systemctl start --no-block'))
        self.assertIn('fail "Invalid or unreadable $connection', provision)

    def test_payload_shell_syntax(self):
        subprocess.run(["bash", "-n", str(PAYLOAD / "provision-nagametv.sh")], check=True)
        subprocess.run(["sh", "-n", str(PAYLOAD / "start-kiosk")], check=True)

    def test_provision_checks_and_installs_the_audio_payload_before_start(self):
        provision = (PAYLOAD / "provision-nagametv.sh").read_text()
        for name, destination in (
            (RECOVERY_SCRIPT, "/usr/local/lib/nagametv-appliance/"),
            (RECOVERY_SERVICE, "/etc/systemd/system/"),
            (WIREPLUMBER_CONF, "/etc/wireplumber/wireplumber.conf.d/"),
            (PULSE_CONF, "/etc/pipewire/pipewire-pulse.conf.d/"),
        ):
            with self.subTest(name=name):
                self.assertIn(name, provision[:provision.index('done\n')])
                self.assertIn(destination + name, provision[:provision.index('systemctl daemon-reload')])
        kiosk = (PAYLOAD / "nagametv-kiosk.service").read_text()
        service = (PAYLOAD / RECOVERY_SERVICE).read_text()
        self.assertIn(RECOVERY_SERVICE, next(line for line in kiosk.splitlines() if line.startswith("Wants=")))
        self.assertIn("BindsTo=nagametv-kiosk.service\n", service)
        self.assertIn("User=nagametv\n", service)

    @unittest.skipUnless(shutil.which("spa-json-dump"), "spa-json-dump is required to parse audio configuration")
    def test_audio_rules_keep_playback_on_the_recreated_hdmi0_sink(self):
        alsa, pulse = (json.loads(subprocess.check_output(["spa-json-dump", str(PAYLOAD / name)]))
                       for name in (WIREPLUMBER_CONF, PULSE_CONF))
        self.assertIn({
            "matches": [{"api.alsa.card.name": "vc4-hdmi-0", "media.class": "Audio/Sink"}],
            "actions": {"update-props": {"node.name": hdmi_audio.SINK}},
        }, alsa["monitor.alsa.rules"])
        self.assertIn({
            "matches": [{"media.class": "Stream/Output/Audio", "target.object": hdmi_audio.SINK}],
            "actions": {"update-props": {"node.dont-fallback": True, "node.dont-move": True, "node.linger": True}},
        }, pulse["stream.rules"])

    def test_start_kiosk_uses_hdmi_even_after_the_readiness_wait_expires(self):
        for available in (True, False):
            with self.subTest(available=available), tempfile.TemporaryDirectory() as directory:
                commands = Path(directory)
                for name, script in {
                    "wlr-randr": 'exit "$MODE_STATUS"',
                    "timeout": 'shift\nexec "$@"',
                    "pactl": 'echo "$*" >> "$COMMAND_LOG"\n'
                             'echo "No such entity" >&2\nexit "$PULSE_STATUS"',
                    "sleep": 'exit 0',
                    "nagametv": 'printf "%s\\n" "$*" "$PULSE_SERVER" "$PULSE_SINK" "$NAGAMETV_AUDIO_SINK"',
                }.items():
                    command = commands / name
                    command.write_text(f"#!/bin/sh\n{script}\n")
                    command.chmod(0o755)
                command_log = commands / "commands.log"
                result = subprocess.run(["/bin/sh", str(PAYLOAD / "start-kiosk")],
                                        env=dict(PATH=directory, XDG_RUNTIME_DIR=directory,
                                                 WAYLAND_DISPLAY="mock", COMMAND_LOG=str(command_log),
                                                 MODE_STATUS="0" if available else "1",
                                                 PULSE_STATUS="0" if available else "1"),
                                        check=True, text=True, capture_output=True, timeout=5)
                self.assertEqual(result.stdout.splitlines()[-4:],
                                 ["--kiosk", f"unix:{directory}/pulse/native", hdmi_audio.SINK, "pulsesink"])
                self.assertEqual(command_log.read_text().splitlines(),
                                 [f"get-sink-volume {hdmi_audio.SINK}"] * (1 if available else 5))
                if available:
                    self.assertEqual(result.stderr, "")
                else:
                    self.assertIn("could not set the HDMI-A-1 mode", result.stderr)
                    self.assertIn("HDMI0 sink unavailable after startup wait (No such entity)", result.stderr)

    def test_connector_requires_hdmi0_to_be_connected_enabled_and_have_edid(self):
        with tempfile.TemporaryDirectory() as directory:
            drm = Path(directory)
            for name in ("card0-HDMI-A-1", "card0-HDMI-A-2"):
                connector = drm / name
                connector.mkdir()
                (connector / "enabled").write_text("enabled\n")
                (connector / "status").write_text("connected\n")
                (connector / "edid").write_bytes(b"EDID")
            hdmi0 = drm / "card0-HDMI-A-1"
            for field, unavailable, ready in (
                ("status", b"disconnected\n", b"connected\n"),
                ("enabled", b"disabled\n", b"enabled\n"),
                ("edid", b"", b"EDID"),
            ):
                with self.subTest(field=field):
                    (hdmi0 / field).write_bytes(unavailable)
                    self.assertFalse(hdmi_audio.connector_ready(drm))
                    (hdmi0 / field).write_bytes(ready)
                    self.assertTrue(hdmi_audio.connector_ready(drm))

    def test_recovery_is_bounded_until_a_new_hdmi_connection_or_sink(self):
        clock = Mock(return_value=0)
        recovery = hdmi_audio.Recovery(clock)
        with (patch.object(hdmi_audio, "run", return_value='[{"name":"auto_null","index":1}]') as run,
              patch.object(hdmi_audio, "connector_ready", return_value=False) as ready,
              redirect_stderr(io.StringIO()) as journal):
            restart = call("systemctl", "--user", "restart", "wireplumber.service")
            recovery.check()
            ready.return_value = True
            clock.return_value = 2
            recovery.check()
            clock.return_value = 3
            recovery.check()
            self.assertNotIn(restart, run.call_args_list)
            for now, expected in ((4, 1), (9, 1), (10, 2), (16, 3), (22, 3), (100, 3)):
                clock.return_value = now
                recovery.check()
                self.assertEqual(run.call_args_list.count(restart), expected)
            self.assertIn("sink still missing after 3 attempts", journal.getvalue())
            ready.return_value = False
            clock.return_value = 102
            recovery.check()
            ready.return_value = True
            clock.return_value = 104
            recovery.check()
            clock.return_value = 106
            recovery.check()
            self.assertEqual(run.call_args_list.count(restart), 4)

    def test_recovery_selects_and_sets_volume_on_each_new_hdmi_sink(self):
        recovery = hdmi_audio.Recovery()
        with (patch.object(hdmi_audio, "run") as run,
              patch.object(hdmi_audio, "connector_ready") as ready,
              redirect_stderr(io.StringIO()) as journal):
            for index in (42, 42, 55):
                run.return_value = json.dumps([{"name": "auto_null", "index": 1},
                                               {"name": hdmi_audio.SINK, "index": index}])
                recovery.check()
            observe = call("pactl", "--format=json", "list", "sinks")
            default = call("pactl", "set-default-sink", hdmi_audio.SINK)
            self.assertEqual(run.call_args_list, [
                observe, call("pactl", "set-sink-volume", "42", "100%"), default,
                observe,
                observe, call("pactl", "set-sink-volume", "55", "100%"), default,
            ])
            ready.assert_not_called()
            self.assertIn("sink nagametv-hdmi0 is ready", journal.getvalue())

    def test_pulse_connection_failure_is_logged_without_restarting_wireplumber(self):
        error = subprocess.CalledProcessError(1, "pactl", stderr="Connection refused")
        with (patch.object(hdmi_audio, "run", side_effect=error) as run,
              patch.object(hdmi_audio, "connector_ready") as ready,
              patch.dict(os.environ),
              patch.object(hdmi_audio.time, "sleep", side_effect=InterruptedError),
              redirect_stderr(io.StringIO()) as journal):
            with self.assertRaises(InterruptedError):
                hdmi_audio.main()
            run.assert_called_once_with("pactl", "--format=json", "list", "sinks")
            ready.assert_not_called()
            self.assertIn("Connection refused", journal.getvalue())


if __name__ == "__main__":
    unittest.main()
