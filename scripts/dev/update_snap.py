"""Install a checksummed NagameTV CI snap on a device over SSH."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile

from scripts.packaging.metadata import Architecture, validate_version


SNAP = "nagametv"
PRESERVED_PLUGS = {f"{SNAP}:gpu-2604", f"{SNAP}:hdmi-cec"}


def run(command: list[str], *, capture: bool = False) -> str:
    result = subprocess.run(
        command, check=True, text=True, stdout=subprocess.PIPE if capture else None
    )
    return result.stdout if capture else ""


def ssh(target: str, *command: str, capture: bool = False, sudo: bool = False) -> str:
    user, host = target.rsplit("@", 1)
    destination = f"{user}@{host.strip('[]')}"
    options = []
    if sudo:
        if sys.stdin.isatty():
            options = ["-t"]
            command = ("sudo", *command)
        else:
            command = ("sudo", "-n", *command)
    return run(
        ["ssh", *options, "--", destination, shlex.join(("env", "LC_ALL=C", *command))],
        capture=capture,
    )


def select_run(args: argparse.Namespace) -> str:
    if args.run is not None:
        return str(args.run)

    options = []
    if args.pr is not None:
        pr = json.loads(run(
            ["gh", "pr", "view", str(args.pr), "--json", "headRefName,headRefOid"],
            capture=True,
        ))
        branch = pr["headRefName"]
        options = ["--commit", pr["headRefOid"], "--event", "pull_request"]
        source = f"PR #{args.pr} at {pr['headRefOid']}"
    else:
        branch = args.branch
        if branch is None:
            branch = run(["git", "branch", "--show-current"], capture=True).strip()
            if not branch:
                raise ValueError("No current branch. Specify --branch, --pr, or --run.")
        source = f"branch {branch!r}"

    run_id = run(
        ["gh", "run", "list", "--workflow", "snap.yml", f"--branch={branch}",
         "--status", "success", "--limit", "1", "--json", "databaseId",
         "--jq", ".[0].databaseId // empty", *options],
        capture=True,
    ).strip()
    if not re.fullmatch(r"[1-9][0-9]*", run_id):
        raise ValueError(f"No successful Snap packages run found for {source}.")
    return run_id


def device_architecture(target: str) -> str:
    for line in ssh(target, "snap", "version", capture=True).splitlines():
        fields = line.split()
        if fields[:1] == ["architecture"] and len(fields) == 2:
            if fields[1] in {architecture.debian for architecture in Architecture}:
                return fields[1]
            raise ValueError(f"Unsupported snap architecture on {target}: {fields[1]}")
    raise ValueError(f"'snap version' did not report an architecture on {target}.")


def download(directory: Path, run_id: str, architecture: str) -> tuple[Path, str]:
    name = f"snap-{architecture}"
    print(f"Downloading {name} from CI run {run_id}.", flush=True)
    run(["gh", "run", "download", run_id, "--name", name, "--dir", str(directory)])
    artifacts = list(directory.glob("*.snap"))
    if len(artifacts) != 1:
        raise ValueError(f"Expected one .snap in {name} from CI run {run_id}.")

    artifact = artifacts[0]
    match = re.fullmatch(rf"{SNAP}_(.+)_{architecture}\.snap", artifact.name)
    if match is None:
        raise ValueError(f"Unexpected {name} archive name: {artifact.name}")
    validate_version(match[1])
    checksum = artifact.with_suffix(".snap.sha256")
    if not checksum.is_file():
        raise ValueError(f"Missing checksum {checksum.name} in {name} from CI run {run_id}.")
    fields = checksum.read_text().split()
    if len(fields) != 2 or fields[1] != artifact.name or not re.fullmatch(r"[0-9a-f]{64}", fields[0]):
        raise ValueError(f"Invalid checksum file {checksum.name}.")
    with artifact.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    if digest != fields[0]:
        raise ValueError(f"SHA-256 mismatch for {artifact.name}.")
    return artifact, digest


def revision(target: str) -> str:
    for line in ssh(target, "snap", "list", SNAP, capture=True).splitlines():
        fields = line.split()
        if fields[:1] == [SNAP] and len(fields) >= 3 and re.fullmatch(r"x?[1-9][0-9]*", fields[2]):
            return fields[2]
    raise ValueError(f"'snap list {SNAP}' did not report an installed revision on {target}.")


def connections(target: str) -> dict[tuple[str, str], str]:
    output = ssh(target, "snap", "connections", SNAP, capture=True)
    lines = output.splitlines()
    if not lines:
        return {}
    if lines[0].split() != ["Interface", "Plug", "Slot", "Notes"]:
        raise ValueError(f"Unexpected 'snap connections {SNAP}' output on {target}.")
    connected = {}
    for line in lines[1:]:
        fields = line.split(maxsplit=3)
        if len(fields) != 4:
            raise ValueError(f"Unexpected snap connection row on {target}: {line!r}")
        _, plug, slot, notes = fields
        if plug != "-" and slot != "-":
            connected[plug, slot] = notes
    return connected


def install(target: str, artifact: Path, digest: str) -> None:
    staging = ssh(target, "mktemp", "-d", "/tmp/nagametv-snap.XXXXXXXXXX", capture=True).strip()
    if not re.fullmatch(r"/tmp/nagametv-snap\.[A-Za-z0-9]{10}", staging):
        raise ValueError(f"Unexpected staging path on {target}: {staging!r}")
    try:
        run(["scp", "--", str(artifact), f"{target}:{staging}/"])
        remote = f"{staging}/{artifact.name}"
        remote_digest = ssh(target, "sha256sum", "--", remote, capture=True).split()
        if remote_digest != [digest, remote]:
            raise ValueError(f"SHA-256 mismatch for the uploaded snap on {target}.")

        previous = revision(target)
        before = connections(target)
        preserve = {
            (plug, slot) for (plug, slot), notes in before.items()
            if "manual" in notes.split(",") or plug in PRESERVED_PLUGS
        }
        ssh(target, "snap", "install", "--dangerous", remote, sudo=True)
        print(f"{target} now runs {SNAP} revision {revision(target)}.", flush=True)
        print(f"To revert on {target}, run: sudo snap revert --revision={previous} {SNAP}", flush=True)

        missing = preserve - connections(target).keys()
        for plug, slot in sorted(missing):
            print(f"Restoring {plug} -> {slot}.", flush=True)
            ssh(target, "snap", "connect", plug, slot, sudo=True)
        if missing:
            remaining = missing - connections(target).keys()
            if remaining:
                endpoints = ", ".join(f"{plug} -> {slot}" for plug, slot in sorted(remaining))
                raise ValueError(f"Connections still missing on {target}: {endpoints}")
        print(f"Verified {len(preserve)} existing connections on {target}.", flush=True)
    finally:
        updating_failed = sys.exc_info()[0] is not None
        try:
            ssh(target, "rm", "-r", "--", staging)
        except (OSError, subprocess.CalledProcessError) as error:
            if not updating_failed:
                raise
            print(f"Could not remove {target}:{staging}: {error}", file=sys.stderr)


def positive_number(value: str) -> int:
    if not re.fullmatch(r"[1-9][0-9]*", value):
        raise argparse.ArgumentTypeError("expected a positive number")
    return int(value)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", metavar="USER@HOST")
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--run", type=positive_number, metavar="ID", help="use a specific CI run")
    source.add_argument(
        "--pr", type=positive_number, metavar="NUMBER",
        help="use the latest successful run for the PR head",
    )
    source.add_argument(
        "--branch", help="use the latest successful run of this branch (default: current branch)",
    )
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_.-]+@(?:[A-Za-z0-9_.-]+|\[[A-Za-z0-9_.:%-]+\])", args.target):
        parser.error("target must be USER@HOST (enclose IPv6 addresses in brackets)")
    try:
        architecture = device_architecture(args.target)
        run_id = select_run(args)
        with tempfile.TemporaryDirectory(prefix="nagametv-snap-") as directory:
            artifact, digest = download(Path(directory), run_id, architecture)
            install(args.target, artifact, digest)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Snap update: {error}\n")


if __name__ == "__main__":
    main()
