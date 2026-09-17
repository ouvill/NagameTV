#!/usr/bin/env bash
set -euo pipefail

# Linux-only packaging: linuxdeploy collects ELF dependencies and Qt resources.
# Compilation and packaging do not open a display, GPU or audio device.
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
output_dir="$project_dir/build/appimage"
build_dir=${APPIMAGE_BUILD_DIR:-$project_dir/build}
tools_dir="$output_dir/tools"
app_id=io.github.ouvill.litv

usage() {
  echo "Usage: scripts/build-appimage.sh"
  echo "Environment: APPIMAGE_BUILD_DIR (default: build), APPIMAGE_BUILD_JOBS (default: 8), QMAKE (default: qmake6)"
}
if [[ $# -gt 0 ]]; then
  if [[ $# == 1 && $1 == --help ]]; then usage; exit 0; fi
  usage >&2
  exit 2
fi
fail() { echo "AppImage: $*" >&2; exit 1; }
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || fail "Only native Linux x86_64 builds are supported."
build_jobs=${APPIMAGE_BUILD_JOBS:-8}
[[ $build_jobs =~ ^[1-9][0-9]*$ ]] || fail "APPIMAGE_BUILD_JOBS must be a positive integer."
for tool in cmake cargo pkg-config curl python3 sha256sum flock desktop-file-validate; do
  command -v "$tool" >/dev/null || fail "Required tool not found: $tool"
done
export QMAKE=${QMAKE:-qmake6}
command -v "$QMAKE" >/dev/null || fail "Qt 6 qmake not found: $QMAKE"
[[ $("$QMAKE" -query QT_VERSION) == 6.* ]] || fail "QMAKE must select Qt 6."
pkg-config --atleast-version=6.8 Qt6Core || fail "Qt 6.8 or newer is required."
pkg-config --atleast-version=1.24 gstreamer-1.0 || fail "GStreamer 1.24 or newer is required."
python3 -c 'import tomllib' || fail "Python 3.11 or newer is required."
for source in third_party/libaribcaption/CMakeLists.txt third_party/tsreadex/CMakeLists.txt; do
  [[ -f $project_dir/$source ]] || fail "Initialize submodules first: git submodule update --init"
done

gst_plugins_dir=$(pkg-config --variable=pluginsdir gstreamer-1.0)
gst_scanner="$(pkg-config --variable=pluginscannerdir gstreamer-1.0)/gst-plugin-scanner"
[[ -x $gst_scanner ]] || fail "GStreamer plugin scanner not found: $gst_scanner"
gst_plugins=()
while IFS= read -r plugin; do
  [[ -z $plugin || $plugin == \#* ]] && continue
  plugin_path="$gst_plugins_dir/libgst$plugin.so"
  [[ -f $plugin_path ]] || fail "Required GStreamer plugin not found: $plugin_path (see docs/appimage.md)"
  gst_plugins+=("$plugin_path")
done < "$project_dir/packaging/appimage/gstreamer-plugins.txt"
qt_plugins_dir=$("$QMAKE" -query QT_INSTALL_PLUGINS)
for plugin in platforms/libqxcb.so imageformats/libqsvg.so imageformats/libqjpeg.so imageformats/libqwebp.so; do
  [[ -f $qt_plugins_dir/$plugin ]] || fail "Required Qt plugin not found: $qt_plugins_dir/$plugin"
done

mkdir -p "$tools_dir"
exec 9>"$output_dir/build.lock"
flock -n 9 || fail "Another AppImage build is running."

# Verify cached downloads too. Releases and hashes are pinned, including the
# runtime, so appimagetool never silently downloads a newer runtime.
download() {
  local filename=$1 url=$2 checksum=$3
  local destination="$tools_dir/$filename"
  if [[ ! -f $destination ]]; then
    curl --fail --location --retry 3 --output "$destination.part" "$url"
    echo "$checksum  $destination.part" | sha256sum --check --status || fail "Checksum mismatch: $filename"
    mv -- "$destination.part" "$destination"
  fi
  echo "$checksum  $destination" | sha256sum --check --status || fail "Checksum mismatch: $destination; remove it and retry."
  chmod +x "$destination"
}
download linuxdeploy-x86_64.AppImage \
  https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage \
  c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d
download linuxdeploy-plugin-qt-x86_64.AppImage \
  https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/1-alpha-20250213-1/linuxdeploy-plugin-qt-x86_64.AppImage \
  15106be885c1c48a021198e7e1e9a48ce9d02a86dd0a1848f00bdbf3c1c92724
download runtime-x86_64 \
  https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64 \
  2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d

app_version=$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["package"]["version"])' "$project_dir/rust/Cargo.toml")
bundle_name="mirakurun-viewer-$app_version-x86_64.AppImage"
export CARGO_BUILD_JOBS="$build_jobs"
cmake -S "$project_dir" -B "$build_dir"
cmake --build "$build_dir" --parallel "$build_jobs"

# A fresh staging directory prevents removed plugins from surviving a rebuild.
# The previous successful AppImage is retained until the new image is ready.
stage_dir=$(mktemp -d "$output_dir/staging.XXXXXX")
trap 'rm -rf -- "$stage_dir"' EXIT
app_dir="$stage_dir/AppDir"
DESTDIR="$app_dir" cmake --install "$build_dir" --prefix /usr
install -d "$app_dir/usr/lib/gstreamer-1.0" "$app_dir/usr/libexec/gstreamer-1.0"
install -m 644 "${gst_plugins[@]}" "$app_dir/usr/lib/gstreamer-1.0/"
install -m 755 "$gst_scanner" "$app_dir/usr/libexec/gstreamer-1.0/"

# Match rust/build.rs: only production QML files, excluding QtTest fixtures.
mkdir "$stage_dir/qml"
cp "$project_dir"/rust/qml/*.qml "$stage_dir/qml/"
export QML_SOURCES_PATHS="$stage_dir/qml"
export EXTRA_QT_MODULES=svg
export APPIMAGE_EXTRACT_AND_RUN=1
export PATH="$tools_dir:$PATH"

"$tools_dir/linuxdeploy-x86_64.AppImage" --appdir "$app_dir" \
  --deploy-deps-only "$app_dir/usr/lib/gstreamer-1.0" \
  --deploy-deps-only "$app_dir/usr/libexec/gstreamer-1.0" \
  --desktop-file "$project_dir/packaging/linux/$app_id.desktop" \
  --icon-file "$project_dir/packaging/linux/$app_id.svg" \
  --custom-apprun "$project_dir/packaging/appimage/AppRun" --plugin qt

# Do not ship a format choice without its runtime encoder. linuxdeploy-qt
# collects the installed imageformats plugins and their ELF dependencies.
for codec in jpeg webp; do
  [[ -n $(find "$app_dir/usr" -name "libq$codec.so" -print -quit) ]] || fail "Missing bundled Qt $codec encoder"
done
desktop-file-validate "$app_dir/$app_id.desktop"
export ARCH=x86_64
export LDAI_VERSION="$app_version"
export LDAI_RUNTIME_FILE="$tools_dir/runtime-x86_64"
export LDAI_OUTPUT="$stage_dir/$bundle_name"
"$tools_dir/linuxdeploy-x86_64.AppImage" --appdir "$app_dir" --output appimage

mv -- "$stage_dir/$bundle_name" "$output_dir/$bundle_name"
(cd "$output_dir" && sha256sum "$bundle_name" > "$bundle_name.sha256")
echo "Package: $output_dir/$bundle_name"
