#!/usr/bin/env python3
"""Build a pinned real EPGStation API service and exercise the production client.

Linux Docker, Python stdlib, and the normal Rust build prerequisites are required.
No display, tuner, GPU, audio, provider API mocks, or user database are used.
"""

import argparse
import hashlib
import http.cookiejar
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "tests/fixtures/epgstation"
PROVIDER = ROOT / "tests/epgstation"
MEDIA = ("recording-seek.ts", "media-h264.mp4", "media-hevc.mkv")
STARTUP_TIMEOUT = 60
CREDENTIALS = {"name": "viewer", "password": "fixture-password"}
REQUIRED_TESTS = (
    "catalogue_pagination_metadata_and_file_bytes",
    "authentication_rejects_invalid_credentials_and_scopes_tokens",
    "cancelled_and_fragmented_logins_leave_the_provider_usable",
    "startup_fixture_matches_real_provider_responses",
)


def command(*args, **kwargs):
    return subprocess.run(args, check=True, cwd=ROOT, **kwargs)


def docker_output(*args):
    result = subprocess.run(["docker", *args], capture_output=True, text=True, cwd=ROOT, timeout=30)
    if result.returncode:
        raise RuntimeError(f"docker {args[0]} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def request(opener, url, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    try:
        with opener.open(req, timeout=3) as response:
            if response.status != 200:
                raise RuntimeError(f"provider request returned HTTP {response.status}")
            return json.load(response)
    except urllib.error.HTTPError as error:
        # Error URLs may contain credentials/tokens; only report the status.
        raise RuntimeError(f"provider request returned HTTP {error.code}") from None


def ready(name, endpoint, authenticated):
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    deadline = time.monotonic() + STARTUP_TIMEOUT
    while time.monotonic() < deadline:
        if docker_output("inspect", "--format", "{{.State.Running}}", name) != "true":
            raise RuntimeError("EPGStation exited during startup; see provider log")
        try:
            status = request(opener, endpoint + "/api/auth")
            if status.get("enabled") is not authenticated:
                raise RuntimeError("EPGStation readiness response has the wrong auth mode")
            return
        except (urllib.error.URLError, TimeoutError, ConnectionError):
            # Refused connections before the HTTP listener opens are normal
            # readiness polling, not retries of a failed test.
            time.sleep(0.1)
    raise RuntimeError(f"EPGStation did not become ready within {STARTUP_TIMEOUT}s")


def capture(endpoint, authenticated, update):
    opener = urllib.request.build_opener(
        urllib.request.ProxyHandler({}),
        urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()),
    )
    if authenticated:
        request(opener, endpoint + "/api/auth/setup", CREDENTIALS)
    query = urllib.parse.urlencode({
        "isHalfWidth": "false", "offset": 0, "limit": 50,
        "keyword": "EPGStation recording",
    })
    routes = {
        "recorded.json": "/api/recorded?" + query,
        "channels.json": "/api/channels",
        "video-124-metadata.json": "/api/videos/124/metadata",
    }
    for filename, route in routes.items():
        actual = request(opener, endpoint + route)
        target = FIXTURES / filename
        if update and not authenticated:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(json.dumps(actual, ensure_ascii=False, indent=2) + "\n")
        else:
            if not target.exists() or json.loads(target.read_text()) != actual:
                raise RuntimeError(f"live provider differs from {target.relative_to(ROOT)}; review before --update-fixtures")


def cargo_test(args, endpoints, run_dir):
    env = os.environ.copy()
    env.update({
        "SQLX_OFFLINE": "true",
        "NAGAMETV_EPGSTATION_ANONYMOUS": endpoints["anonymous"],
        "NAGAMETV_EPGSTATION_AUTHENTICATED": endpoints["password"],
    })
    cargo = ["cargo", "test", "--manifest-path", "rust/Cargo.toml", "--release", "--locked",
             "epgstation::provider_tests::", "--", "--ignored", "--nocapture"]
    client_container = None
    client_exit_code = 0
    if args.client_image:
        # Linux CI runs Cargo in the existing build image. Host networking lets
        # the client reach the ephemeral ports on the runner's loopback.
        name = "epgstation-contract-client-" + uuid.uuid4().hex
        prefix = ["create", "--name", name, "--network", "host", "--user", f"{os.getuid()}:{os.getgid()}",
                  "--workdir", "/project", "--mount", f"type=bind,src={ROOT},dst=/project"]
        for key in ("SQLX_OFFLINE", "NAGAMETV_EPGSTATION_ANONYMOUS", "NAGAMETV_EPGSTATION_AUTHENTICATED",
                    "CARGO_HOME", "CARGO_TARGET_DIR", "CARGO_BUILD_JOBS"):
            if key in env:
                prefix += ["--env", f"{key}={env[key]}"]
        docker_output(*prefix, args.client_image, *cargo)
        client_container = name
        cargo = ["docker", "start", "--attach", name]
    try:
        with (run_dir / "cargo.log").open("w") as log:
            result = subprocess.run(cargo, cwd=ROOT, env=env, timeout=1800,
                                    stdout=log, stderr=subprocess.STDOUT)
        if client_container:
            client_exit_code = int(docker_output("inspect", "--format", "{{.State.ExitCode}}", client_container))
    finally:
        if client_container:
            # Also remove a still-running client when Docker's attach command
            # times out; killing the Docker CLI alone does not stop its container.
            docker_output("rm", "--force", client_container)
    output = (run_dir / "cargo.log").read_text()
    print(output, end="", flush=True,
          file=sys.stderr if result.returncode or client_exit_code else sys.stdout)
    if client_exit_code:
        raise RuntimeError(f"Rust client container failed with exit code {client_exit_code}; see cargo.log")
    result.check_returncode()
    for name in REQUIRED_TESTS:
        if f"test epgstation::provider_tests::{name} ... ok" not in output:
            raise RuntimeError(f"required real-provider test did not pass: {name}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--client-image", help="run Cargo in this existing Linux build image (CI)")
    parser.add_argument("--update-fixtures", action="store_true", help="replace JSON snapshots with live anonymous responses; review the diff")
    args = parser.parse_args()
    docker_output("info", "--format", "{{.ServerVersion}}")
    pin = json.loads((PROVIDER / "provider.json").read_text())
    output = ROOT / "build/epgstation-contract"
    output.mkdir(parents=True, exist_ok=True)
    run_dir = Path(tempfile.mkdtemp(prefix="run-", dir=output))
    print(f"EPGStation contract logs: {run_dir}", flush=True)
    with tempfile.TemporaryDirectory(prefix="epgstation-build-") as context:
        context = Path(context)
        for filename in ("Dockerfile", "bootstrap.cjs"):
            shutil.copyfile(PROVIDER / filename, context / filename)
        (context / "media").mkdir()
        for filename in MEDIA:
            shutil.copyfile(ROOT / "tests/fixtures" / filename, context / "media" / filename)
        digest = hashlib.sha256((PROVIDER / "provider.json").read_bytes())
        for file in sorted(context.rglob("*")):
            if file.is_file():
                digest.update(file.read_bytes())
        image = "nagametv-epgstation-contract:" + digest.hexdigest()[:16]
        with (run_dir / "build.log").open("w") as log:
            command("docker", "build", "--tag", image,
                    "--iidfile", str(run_dir / "image-id"),
                    "--build-arg", f"NODE_IMAGE={pin['node_image']}",
                    "--build-arg", f"SOURCE_COMMIT={pin['commit']}", str(context),
                    stdout=log, stderr=subprocess.STDOUT, timeout=1200)
    # Pin this run to the image just built, even if another run updates the tag.
    image_id = (run_dir / "image-id").read_text().strip()
    network = "epgstation-contract-" + uuid.uuid4().hex
    containers = []
    docker_output("network", "create", network)
    try:
        endpoints = {}
        for mode in ("anonymous", "password"):
            name = network + "-" + mode
            docker_output("create", "--name", name, "--network", network,
                          "--publish", "127.0.0.1::8888", "--env", f"CONTRACT_AUTH={mode}", image_id)
            containers.append(name)
            docker_output("start", name)
            port = docker_output("port", name, "8888/tcp")
            if not port.startswith("127.0.0.1:") or "\n" in port:
                raise RuntimeError("provider must publish exactly one loopback endpoint")
            endpoint = "http://" + port + ("/protected" if mode == "password" else "")
            ready(name, endpoint, mode == "password")
            capture(endpoint, mode == "password", args.update_fixtures)
            endpoints[mode] = endpoint
            print(f"Real EPGStation ready: {mode}; JSON snapshots verified", flush=True)
        cargo_test(args, endpoints, run_dir)
    finally:
        cleanup_failed = False
        for name in reversed(containers):
            with (run_dir / f"{name.rsplit('-', 1)[-1]}.log").open("w") as log:
                result = subprocess.run(["docker", "logs", name], stdout=log, stderr=subprocess.STDOUT)
            if result.returncode:
                print(f"Failed to collect provider logs for {name}", file=sys.stderr)
                cleanup_failed = True
            result = subprocess.run(["docker", "rm", "--force", name], capture_output=True, text=True)
            if result.returncode:
                print(f"Failed to remove test container {name}: {result.stderr}", file=sys.stderr)
                cleanup_failed = True
        result = subprocess.run(["docker", "network", "rm", network], capture_output=True, text=True)
        if result.returncode:
            print(f"Failed to remove test network: {result.stderr}", file=sys.stderr)
            cleanup_failed = True
        if cleanup_failed:
            raise RuntimeError("EPGStation test resource cleanup failed")
    (run_dir / "result.json").write_text(json.dumps({
        "provider": pin, "image_id": image_id,
        "modes": list(endpoints), "result": "passed",
    }, indent=2) + "\n")
    print("Real EPGStation API contract passed (anonymous and authenticated).", flush=True)


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"EPGStation contract failed: {error}", file=sys.stderr)
        sys.exit(1)
