#!/usr/bin/env bash
# Extracts the version from Cargo.toml, compares against the latest git tag,
# and outputs whether a release should be made.
#
# Outputs:
#   should_release=true|false
#   version=<version string>

set -euo pipefail

# Extract version from Cargo.toml using sed (no jq dependency required).
extract_cargo_version() {
    local cargo_path="${1:-Cargo.toml}"
    if [[ ! -f "$cargo_path" ]]; then
        echo "ERROR: $cargo_path not found" >&2
        exit 1
    fi
    local version
    version=$(sed -n 's/^version *= *"\([^"]*\)"/\1/p' "$cargo_path" | head -1)
    if [[ -z "$version" ]]; then
        echo "ERROR: could not extract version from $cargo_path" >&2
        exit 1
    fi
    echo "$version"
}

get_latest_tag() {
    # Returns the latest tag in the form v<version>, stripped of the leading 'v'.
    # Returns empty string if no tags exist.
    local latest_tag
    latest_tag=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
    if [[ -n "$latest_tag" ]]; then
        # Strip leading 'v' prefix
        echo "${latest_tag#v}"
    else
        echo ""
    fi
}

main() {
    local cargo_path="${1:-Cargo.toml}"

    local cargo_version
    cargo_version=$(extract_cargo_version "$cargo_path")

    local latest_version
    latest_version=$(get_latest_tag)

    # If no tags exist, this is the first release — should release.
    # If the cargo version differs from the latest tag, should release.
    if [[ -z "$latest_version" ]] || [[ "$cargo_version" != "$latest_version" ]]; then
        echo "should_release=true"
    else
        echo "should_release=false"
    fi
    echo "version=$cargo_version"
}

main "$@"