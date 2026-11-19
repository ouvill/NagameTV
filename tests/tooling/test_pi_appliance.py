"""Validate the Raspberry Pi boot media payload without starting services."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import unittest
from xml.etree import ElementTree

try:
    import yaml
except ModuleNotFoundError:
    yaml = None


PAYLOAD = Path(__file__).resolve().parents[2] / "packaging/pi-appliance"
NETWORK_CONFIGS = ("network-config", "network-config.wifi")


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
            unit = units / "nagametv-kiosk.service"
            shutil.copyfile(PAYLOAD / unit.name, unit)
            for name in ("sysinit", "basic", "shutdown", "multi-user"):
                (units / f"{name}.target").write_text("[Unit]\nDefaultDependencies=no\n")
            # verify checks executable paths in the offline root. It never
            # starts the compositor, changes the VT, or connects to host systemd.
            for name in ("labwc", "chvt"):
                command = root / "usr/bin" / name
                command.parent.mkdir(parents=True, exist_ok=True)
                command.touch(mode=0o755)
            result = subprocess.run(["systemd-analyze", f"--root={root}", "--man=no",
                                     "--generators=no", "--recursive-errors=yes", "verify", unit.name],
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


if __name__ == "__main__":
    unittest.main()
