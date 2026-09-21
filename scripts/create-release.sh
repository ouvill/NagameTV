#!/usr/bin/env bash
set -euo pipefail
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
if [[ $# != 2 && $# != 3 ]]; then
  echo "Usage: scripts/create-release.sh v<VERSION> ASSET_DIRECTORY [--latest-build]" >&2
  exit 2
fi
tag=$1
asset_dir=$2
mode=tag
if [[ $# == 3 ]]; then
  if [[ $3 != --latest-build ]]; then
    echo "Expected --latest-build." >&2
    exit 2
  fi
  mode=latest-build
fi
python3 "$project_dir/scripts/check-release-metadata.py" --tag "$tag"
version=${tag#v}

if [[ $mode == latest-build ]]; then
  commit=$(git -C "$project_dir" rev-parse --verify HEAD)
  tag=latest-build
fi
cd -- "$asset_dir"
assets=()
bundles=$(python3 "$project_dir/scripts/package_metadata.py" "$version")
while IFS= read -r bundle; do
  # Validate all bundles before any GitHub writes, including hiding a release.
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

if [[ $mode == latest-build ]]; then
  # The workflow serializes release jobs. Recheck main after taking that lock
  # so an older build or rerun cannot replace a newer published build.
  main_commit=$(gh api "repos/$GH_REPO/git/ref/heads/main" --jq '.object.sha')
  if [[ ! $main_commit =~ ^[0-9a-f]{40}$ ]]; then
    echo "Invalid main commit returned by GitHub." >&2
    exit 1
  fi
  if [[ $commit != "$main_commit" ]]; then
    echo "Skipping outdated build $commit; main is now $main_commit."
    exit 0
  fi

  state=$(gh api --paginate "repos/$GH_REPO/releases?per_page=100" \
    --jq '.[] | select(.tag_name == "latest-build") | if .immutable then "immutable" elif .prerelease then "prerelease" else "release" end')
  case "$state" in
    ''|prerelease) ;;
    *)
      echo "Cannot replace latest-build release in state: $state" >&2
      exit 1
      ;;
  esac
  tag_commit=$(gh api "repos/$GH_REPO/git/matching-refs/tags/$tag" \
    --jq '.[] | select(.ref == "refs/tags/latest-build") | .object.sha')
  previous_assets=''
  if [[ $state == prerelease ]]; then
    previous_assets=$(gh release view "$tag" --json assets --jq '.assets[].name')
    # A partial upload must remain a draft. Rerunning repairs it before publish.
    gh release edit "$tag" --draft=true
  fi
  if [[ -n $tag_commit ]]; then
    gh api --method PATCH "repos/$GH_REPO/git/refs/tags/$tag" \
      -f "sha=$commit" -F force=true
  else
    gh api --method POST "repos/$GH_REPO/git/refs" \
      -f "ref=refs/tags/$tag" -f "sha=$commit"
  fi

  title="Latest main build (${commit:0:12})"
  notes=$(printf 'Automated pre-release build of main.\n\nVersion: %s\nCommit: %s\n' "$version" "$commit")
  case "$state" in
    '')
      gh release create "$tag" "${assets[@]}" --verify-tag --draft --prerelease \
        --latest=false --title "$title" --notes "$notes"
      ;;
    prerelease)
      gh release upload "$tag" "${assets[@]}" --clobber
      while IFS= read -r previous_asset; do
        [[ -n $previous_asset ]] || continue
        for asset in "${assets[@]}"; do
          [[ $previous_asset != "$asset" ]] || continue 2
        done
        gh release delete-asset --yes -- "$tag" "$previous_asset"
      done <<< "$previous_assets"
      ;;
  esac
  gh release edit "$tag" --draft=false --prerelease --latest=false \
    --target "$commit" --title "$title" --notes "$notes"
  exit 0
fi

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
