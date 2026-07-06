#!/usr/bin/env bash
# Updates the Homebrew tap (Fizzizist/homebrew-tap) with the latest formula.
#
# Environment variables:
#   HOMEBREW_TAP_TOKEN — GitHub PAT with repo scope for Fizzizist/homebrew-tap
#
# Arguments:
#   $1 — version string (e.g. 1.5.0)
#   $2 — path to the macOS aarch64 tarball
#   $3 — path to the formula template (packaging/homebrew/reflections-bin.rb)

set -euo pipefail

VERSION="${1:?Usage: update-homebrew.sh <version> <tarball> <formula-template>}"
TARBALL="${2:?}"
FORMULA_TEMPLATE="${3:?}"

if [[ -z "${HOMEBREW_TAP_TOKEN:-}" ]]; then
    echo "ERROR: HOMEBREW_TAP_TOKEN must be set" >&2
    exit 1
fi

if [[ ! -f "$TARBALL" ]] || [[ ! -f "$FORMULA_TEMPLATE" ]]; then
    echo "ERROR: tarball or formula template not found" >&2
    exit 1
fi

SHA256=$(sha256sum "$TARBALL" | awk '{print $1}')

TAP_URL="https://github.com/Fizzizist/reflections/releases/download/v${VERSION}/reflections-v${VERSION}-aarch64-apple-darwin.tar.gz"

WORK_DIR=$(mktemp -d)
cleanup() {
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT

cd "$WORK_DIR"
git clone --quiet "https://x-access-token:${HOMEBREW_TAP_TOKEN}@github.com/Fizzizist/homebrew-tap.git"
cd homebrew-tap

sed "s|__VERSION__|${VERSION}|g; s|__URL__|${TAP_URL}|g; s|__SHA256__|${SHA256}|g" \
    "$FORMULA_TEMPLATE" > Formula/reflections-bin.rb

git add Formula/reflections-bin.rb
git config user.email "github-actions[bot]@users.noreply.github.com"
git config user.name "github-actions[bot]"
git commit --quiet -m "Update reflections-bin to v${VERSION}" || {
    echo "No changes to commit — formula already up to date"
    exit 0
}
git push --quiet origin main

echo "Homebrew tap updated to v${VERSION}"