#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
SHA256="${2:-}"

if [[ -z "$VERSION" || -z "$SHA256" ]]; then
  echo "Usage: $0 <version> <sha256>" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TEMPLATE_FILE="$SCRIPT_DIR/../homebrew/reflections-bin.rb"

CLONE_DIR=$(mktemp -d)

trap 'rm -rf "$CLONE_DIR"' EXIT

git clone "https://${HOMEBREW_TAP_TOKEN}@github.com/Fizzizist/homebrew-tap.git" "$CLONE_DIR"

mkdir -p "$CLONE_DIR/Formula"
FORMULA_FILE="$CLONE_DIR/Formula/reflections-bin.rb"

if [[ ! -f "$FORMULA_FILE" ]]; then
  cp "$TEMPLATE_FILE" "$FORMULA_FILE"
fi

sed -i.bak "s/^  version .*/  version \"$VERSION\"/" "$FORMULA_FILE"
sed -i.bak "s/^  sha256 .*/  sha256 \"$SHA256\"/" "$FORMULA_FILE"
rm -f "${FORMULA_FILE}.bak"

cd "$CLONE_DIR"
git config user.name "Release Bot"
git config user.email "release@reflections.local"
git add Formula/reflections-bin.rb
git commit -m "Update reflections-bin to v${VERSION}"
git push origin HEAD:main
