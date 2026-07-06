#!/usr/bin/env bash
set -euo pipefail

cargo_toml="${1:-Cargo.toml}"
git_dir="${2:-.}"

if [[ ! -f "$cargo_toml" ]]; then
    echo "Error: Cargo.toml not found at $cargo_toml" >&2
    exit 1
fi

version_line=$(grep -E '^version\s*=\s*"[0-9]+\.[0-9]+\.[0-9]+"' "$cargo_toml" 2>/dev/null || true)
if [[ -z "$version_line" ]]; then
    echo "Error: Could not parse version from $cargo_toml" >&2
    exit 1
fi

cargo_version=$(echo "$version_line" | sed -E 's/version\s*=\s*"([^"]+)"/\1/')

latest_tag=$(git -C "$git_dir" describe --tags --abbrev=0 2>/dev/null || echo "")

if [[ -z "$latest_tag" ]]; then
    echo "should_release=true"
    echo "version=$cargo_version"
    exit 0
fi

tag_version="${latest_tag#v}"

if [[ "$cargo_version" == "$tag_version" ]]; then
    echo "should_release=false"
    echo "version=$cargo_version"
else
    echo "should_release=true"
    echo "version=$cargo_version"
fi

exit 0
