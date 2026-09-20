#!/usr/bin/env bash
set -euo pipefail
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
if [[ $# != 2 ]]; then
  echo "Usage: scripts/create-release-draft.sh v<VERSION> ASSET_DIRECTORY" >&2
  exit 2
fi
tag=$1
asset_dir=$2
python3 "$project_dir/scripts/check-release-metadata.py" --tag "$tag"
version=${tag#v}
cd -- "$asset_dir"
assets=()
bundles=$(python3 "$project_dir/scripts/package_metadata.py" "$version")
while IFS= read -r bundle; do
  # Require all bundles and canonical checksums of those exact files before
  # making any API write. A partial build cannot produce a release draft.
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
