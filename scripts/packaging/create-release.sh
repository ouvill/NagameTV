#!/usr/bin/env bash
set -euo pipefail
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
if [[ $# != 2 ]]; then
  echo "Usage: scripts/packaging/create-release.sh v<VERSION> ASSET_DIRECTORY" >&2
  exit 2
fi
tag=$1
asset_dir=$2
PYTHONPATH="$project_dir" python3 -m scripts.packaging.check_release_metadata --tag "$tag"
version=${tag#v}

cd -- "$asset_dir"
assets=()
bundles=$(PYTHONPATH="$project_dir" python3 -m scripts.packaging.metadata "$version")
while IFS= read -r bundle; do
  # Validate all bundles before any GitHub writes.
  [[ -s "$bundle" && -s "$bundle.sha256" ]] || {
    echo "Missing release asset: $bundle or its SHA-256" >&2
    exit 1
  }
  sha256sum "$bundle" | cmp --silent "$bundle.sha256" - || {
    echo "Release checksum mismatch: $bundle" >&2
    exit 1
  }
  assets+=("$bundle" "$bundle.sha256")
done <<< "$bundles"
appimage=$(PYTHONPATH="$project_dir" python3 -m scripts.packaging.metadata "$version" --appimage)
# The zsync header records the SHA-1 of the image it describes; a stale file
# would make update tools reject or mis-patch the published AppImage.
[[ -s "$appimage.zsync" ]] || {
  echo "Missing release asset: $appimage.zsync" >&2
  exit 1
}
zsync_sha1=$(grep --text --max-count=1 '^SHA-1: ' "$appimage.zsync" | cut -d' ' -f2) || zsync_sha1=''
[[ $zsync_sha1 == "$(sha1sum "$appimage" | cut -d' ' -f1)" ]] || {
  echo "AppImage zsync does not match: $appimage.zsync" >&2
  exit 1
}
assets+=("$appimage.zsync")
: "${GH_REPO:?Set GH_REPO to owner/repository}"

# Listing with pagination distinguishes 'no release' from API/auth failures.
# Validated SemVer above cannot inject a jq expression.
draft=$(gh api --paginate "repos/$GH_REPO/releases?per_page=100" \
  --jq ".[] | select(.tag_name == \"$tag\") | .draft")
case "$draft" in
  false)
    echo "Release $tag is already published; refusing to replace its assets." >&2
    exit 1
    ;;
  true)
    gh release upload "$tag" "${assets[@]}" --clobber
    ;;
  '')
    prerelease=()
    if [[ ${version%%+*} == *-* ]]; then prerelease=(--prerelease); fi
    gh release create "$tag" "${assets[@]}" --verify-tag --draft \
      --title "NagameTV $version" --generate-notes "${prerelease[@]}"
    ;;
  *)
    echo "Unexpected release state for $tag: $draft" >&2
    exit 1
    ;;
esac
