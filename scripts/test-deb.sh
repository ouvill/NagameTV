#!/usr/bin/env bash
set -euo pipefail

# All apt/dpkg mutations happen in a disposable container. No host hardware,
# desktop or audio sockets are exposed, and no GUI application is initialized.
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
[[ $# == 1 ]] || { echo "Usage: scripts/test-deb.sh 24.04|26.04" >&2; exit 2; }
case "$1" in
  24.04) image=ubuntu:24.04@sha256:b3cc40b72b93588182b5410f723c7aaf142363311c2aa993d8a453ddcbb3ae15 ;;
  26.04) image=ubuntu:26.04@sha256:da6fc2be547864451aa253836dd926da33623312df4a9a243e35dc877c378a78 ;;
  *) echo "Unsupported Ubuntu release: $1" >&2; exit 2 ;;
esac
docker info >/dev/null
docker run --rm -i --mount "type=bind,src=$project_dir,dst=/project,readonly" \
  --env UBUNTU_RELEASE="$1" --env DEBIAN_FRONTEND=noninteractive \
  "$image" bash -s <<'CHECK'
set -euo pipefail
apt-get update
apt-get install --yes --no-install-recommends python3 desktop-file-utils
version=$(python3 -c 'import tomllib; print(tomllib.load(open("/project/rust/Cargo.toml", "rb"))["package"]["version"])')
bundle=$(PYTHONPATH=/project/scripts python3 -c \
  'import sys; from package_metadata import UbuntuRelease, deb_filename; print(deb_filename(sys.argv[1], UbuntuRelease(sys.argv[2])))' \
  "$version" "$UBUNTU_RELEASE")
cd "/project/build/deb/ubuntu$UBUNTU_RELEASE"
sha256sum --check "$bundle.sha256"
# Minimal Ubuntu images exclude /usr/share/doc by default. Include this
# package's documentation so integrity checks cover its entire payload.
apt-get -o 'Dpkg::Options::=--path-include=/usr/share/doc/nagametv/*' \
  install --yes --no-install-recommends "./$bundle"
python3 /project/scripts/check-installed-deb.py "$UBUNTU_RELEASE"
# dpkg verifies every regular payload file against the generated md5sums.
verification=$(dpkg --verify nagametv)
[[ -z $verification ]] || { echo "$verification" >&2; exit 1; }
apt-get remove --yes nagametv
for path in /usr/bin/nagametv /opt/nagametv \
  /usr/share/applications/io.github.ouvill.nagametv.desktop \
  /usr/share/icons/hicolor/scalable/apps/io.github.ouvill.nagametv.svg \
  /usr/share/metainfo/io.github.ouvill.nagametv.metainfo.xml; do
  [[ ! -e $path ]] || { echo "Package file remains after removal: $path" >&2; exit 1; }
done
echo "Ubuntu $UBUNTU_RELEASE: deb installation, linkage, integrity and removal OK"
CHECK
