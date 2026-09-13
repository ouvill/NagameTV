#!/usr/bin/env bash
set -euo pipefail

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
output_dir="$project_dir/build/flatpak"
manifest="$project_dir/packaging/flatpak/io.github.ouvill.litv.json"
app_id=io.github.ouvill.litv
if [[ $# -gt 0 ]]; then
  echo "Usage: scripts/build-flatpak.sh (optional environment: FLATPAK_BUILD_JOBS)" >&2
  exit 2
fi

for tool in flatpak flatpak-builder python3 eu-strip; do
  command -v "$tool" >/dev/null || { echo "Required tool not found: $tool" >&2; exit 1; }
done
python3 "$project_dir/scripts/flatpak-cargo-sources.py" --check
mkdir -p "$output_dir"
app_version=$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["package"]["version"])' "$project_dir/rust/Cargo.toml")
architecture=$(flatpak --default-arch)
bundle_name="mirakurun-viewer-$app_version-$architecture.flatpak"

flatpak-builder --user --install-deps-from=flathub --assumeyes --force-clean --disable-rofiles-fuse \
  --state-dir="$output_dir/state" --repo="$output_dir/repo" \
  --jobs="${FLATPAK_BUILD_JOBS:-8}" \
  "$output_dir/app" "$manifest"

flatpak build-bundle "$output_dir/repo" "$output_dir/$bundle_name" "$app_id" stable \
  --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo
(cd "$output_dir" && sha256sum "$bundle_name" > "$bundle_name.sha256")
echo "Package: $output_dir/$bundle_name"
