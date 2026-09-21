#!/usr/bin/env bash
set -euo pipefail

# Linux amd64 only. Compilation, AppImage extraction and ELF inspection never
# open a display, GPU or audio device; no host sessions are passed to Docker.
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
usage() {
  echo "Usage: scripts/build-deb.sh 24.04|26.04 [--native] [--appimage FILE]"
  echo "Default: build in Docker for the selected Ubuntu release."
  echo "--native: require that exact Ubuntu release and installed build dependencies."
  echo "--appimage: native 24.04 only; reuse a matching AppImage and its SHA-256."
  echo "Environment: DEB_BUILD_JOBS (default: 8), DEB_BUILD_DIR (native 26.04 only)"
}
fail() { echo "deb: $*" >&2; exit 1; }
if [[ $# == 1 && $1 == --help ]]; then usage; exit 0; fi
[[ $# -ge 1 ]] || { usage >&2; exit 2; }
ubuntu=$1
shift
case "$ubuntu" in 24.04|26.04) ;; *) usage >&2; exit 2 ;; esac
mode=container
appimage=
while [[ $# -gt 0 ]]; do
  case "$1" in
    --native) mode=native; shift ;;
    --appimage) [[ $# -ge 2 ]] || { usage >&2; exit 2; }; appimage=$2; shift 2 ;;
    *) usage >&2; exit 2 ;;
  esac
done
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || fail "Only Linux x86_64 is supported."
build_jobs=${DEB_BUILD_JOBS:-8}
[[ $build_jobs =~ ^[1-9][0-9]*$ ]] || fail "DEB_BUILD_JOBS must be a positive integer."
[[ -z $appimage || ( $mode == native && $ubuntu == 24.04 ) ]] || fail "--appimage requires 24.04 --native."
[[ -z ${DEB_BUILD_DIR:-} || ( $mode == native && $ubuntu == 26.04 ) ]] || fail "DEB_BUILD_DIR requires 26.04 --native."
output_dir="$project_dir/build/deb/ubuntu$ubuntu"
if [[ $mode == container ]]; then
  command -v docker >/dev/null || fail "Docker is required; see docs/deb.md."
  docker info >/dev/null || fail "Docker daemon is unavailable."
  case "$ubuntu" in
    24.04)
      image=nagametv-appimage-ubuntu24:local
      docker build --target appimage --tag "$image" "$project_dir/packaging/appimage"
      options=(--env APPIMAGE_BUILD_DIR=/project/build/appimage/ubuntu24/native
               --env CARGO_HOME=/project/build/appimage/ubuntu24/cargo-home)
      ;;
    26.04)
      image=nagametv-deb-ubuntu26:local
      docker build --tag "$image" "$project_dir/packaging/deb"
      options=(--env CARGO_HOME=/project/build/deb/ubuntu26.04/cargo-home)
      ;;
  esac
  build_source=$(python3 "$project_dir/scripts/build-source-info.py")
  exec docker run --rm --user "$(id -u):$(id -g)" \
    --mount "type=bind,src=$project_dir,dst=/project" \
    --env DEB_BUILD_JOBS="$build_jobs" "${options[@]}" \
    --env NAGAMETV_BUILD_SOURCE="$build_source" --env SOURCE_DATE_EPOCH \
    "$image" bash scripts/build-deb.sh "$ubuntu" --native
fi

# Do not accidentally encode the Workshop/newer Ubuntu ABI into a noble deb.
# shellcheck source=/dev/null
source /etc/os-release
[[ $ID == ubuntu && $VERSION_ID == "$ubuntu" ]] || fail "Requires Ubuntu $ubuntu; found $ID $VERSION_ID. Use Docker mode."
for tool in cmake python3 dpkg-deb dpkg-shlibdeps readelf strip sha256sum flock desktop-file-validate; do
  command -v "$tool" >/dev/null || fail "Required tool not found: $tool"
done
python3 "$project_dir/scripts/check-release-metadata.py"
app_version=$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["package"]["version"])' "$project_dir/rust/Cargo.toml")
mkdir -p "$output_dir"
exec 9>"$output_dir/build.lock"
flock -n 9 || fail "Another Ubuntu $ubuntu deb build is running."
stage_dir=$(mktemp -d "$output_dir/staging.XXXXXX")
trap 'rm -rf -- "$stage_dir"' EXIT
package_dir="$stage_dir/package"
mkdir "$package_dir"
app_id=io.github.ouvill.nagametv
case "$ubuntu" in
  24.04)
    if [[ -z $appimage ]]; then
      APPIMAGE_BUILD_JOBS="$build_jobs" bash "$project_dir/scripts/build-appimage.sh" --native
      appimage="$project_dir/build/appimage/nagametv-$app_version-x86_64.AppImage"
    fi
    appimage=$(realpath -- "$appimage")
    [[ $(basename -- "$appimage") == "nagametv-$app_version-x86_64.AppImage" ]] || fail "AppImage version/architecture does not match."
    [[ -s $appimage && -s $appimage.sha256 ]] || fail "AppImage and SHA-256 are required."
    (cd -- "$(dirname -- "$appimage")" && sha256sum "$(basename -- "$appimage")") \
      | cmp --silent "$appimage.sha256" - || fail "AppImage checksum mismatch."
    (cd "$stage_dir" && "$appimage" --appimage-extract > extract.log)
    python3 "$project_dir/scripts/check-appimage-glibc.py" "$stage_dir/squashfs-root" --max-glibc 2.39
    install -d "$package_dir/opt" "$package_dir/usr/bin"
    mv "$stage_dir/squashfs-root" "$package_dir/opt/nagametv"
    # Use the already tested bundled launcher; /usr/bin preserves cwd/arguments.
    cat > "$package_dir/usr/bin/nagametv" <<'LAUNCHER'
#!/bin/sh
exec /opt/nagametv/AppRun "$@"
LAUNCHER
    chmod 755 "$package_dir/usr/bin/nagametv"
    for resource in "applications/$app_id.desktop" "icons/hicolor/scalable/apps/$app_id.svg" "metainfo/$app_id.metainfo.xml"; do
      install -Dm644 "$package_dir/opt/nagametv/usr/share/$resource" "$package_dir/usr/share/$resource"
    done
    licenses="$package_dir/opt/nagametv/usr/share/licenses/nagametv"
    ;;
  26.04)
    build_dir=${DEB_BUILD_DIR:-$output_dir/native}
    export CARGO_BUILD_JOBS="$build_jobs"
    cmake -S "$project_dir" -B "$build_dir" -DNAGAMETV_DISTRIBUTION=ON
    cmake --build "$build_dir" --parallel "$build_jobs"
    DESTDIR="$package_dir" cmake --install "$build_dir" --prefix /usr
    strip --strip-unneeded "$package_dir/usr/bin/nagametv"
    licenses="$package_dir/usr/share/licenses/nagametv"
    ;;
esac
install -d "$package_dir/usr/share/doc/nagametv"
cp -a "$licenses" "$package_dir/usr/share/doc/nagametv/third-party-licenses"
install -m644 "$project_dir/docs/deb.md" "$package_dir/usr/share/doc/nagametv/README.md"
desktop-file-validate "$package_dir/usr/share/applications/$app_id.desktop"
python3 "$project_dir/scripts/package-deb.py" "$ubuntu" "$package_dir" "$output_dir"
