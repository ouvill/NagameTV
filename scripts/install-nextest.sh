#!/usr/bin/env bash
# Pinned official release archives, verified before extracting or installing.
set -euo pipefail
version=0.9.146
case "$(uname -s)/$(uname -m)" in
    Linux/x86_64)
        target=x86_64-unknown-linux-gnu
        checksum=682c21b777c333e96fd532e114d3a5a894e0729ab88d94c0a9f20f8419695428 ;;
    Linux/aarch64)
        target=aarch64-unknown-linux-gnu
        checksum=b2e33d7c72de7ade0ff7b3a948ac37516b24f8a836b7a8870c1f634a94be9de9 ;;
    *) echo "Unsupported platform; install cargo-nextest $version from source." >&2; exit 1 ;;
esac
destination=${1:-$HOME/.local/bin}
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
curl --fail --silent --show-error --location --retry 3 \
    "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-$version/cargo-nextest-$version-$target.tar.gz" \
    --output "$temporary/nextest.tar.gz"
printf '%s  %s\n' "$checksum" "$temporary/nextest.tar.gz" | sha256sum --check
tar -xzf "$temporary/nextest.tar.gz" -C "$temporary" cargo-nextest
mkdir -p "$destination"
install -m755 "$temporary/cargo-nextest" "$destination/cargo-nextest"
"$destination/cargo-nextest" --version
echo "Installed in $destination; this directory must be in PATH."
