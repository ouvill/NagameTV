#!/usr/bin/env bash
set -euo pipefail

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
output_dir="$project_dir/build/flatpak"
manifest="$project_dir/packaging/flatpak/io.github.ouvill.nagametv.json"
app_id=io.github.ouvill.nagametv
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
bundle_name="nagametv-$app_version-$architecture.flatpak"
build_manifest="$output_dir/build-manifest.json"
python3 "$project_dir/scripts/build-source-info.py" --flatpak-manifest "$manifest" --output "$build_manifest"
epoch_args=()
if [[ -n ${SOURCE_DATE_EPOCH+x} ]]; then
  epoch_args=(--override-source-date-epoch="$SOURCE_DATE_EPOCH")
fi

flatpak-builder --user --install-deps-from=flathub --assumeyes --force-clean --disable-rofiles-fuse \
  --state-dir="$output_dir/state" --repo="$output_dir/repo" \
  --jobs="${FLATPAK_BUILD_JOBS:-8}" \
  "${epoch_args[@]}" "$output_dir/app" "$build_manifest"

flatpak build-bundle "$output_dir/repo" "$output_dir/$bundle_name" "$app_id" stable \
  --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo
(cd "$output_dir" && sha256sum "$bundle_name" > "$bundle_name.sha256")
echo "Package: $output_dir/$bundle_name"
