#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
SHA256="${2:-}"

if [[ -z "$VERSION" || -z "$SHA256" ]]; then
  echo "Usage: $0 <version> <sha256>" >&2
  exit 1
fi

if [[ -z "${DEPLOY_HOST:-}" || -z "${RELEASES_URL_PATH:-}" ]]; then
  echo "Missing required env vars: DEPLOY_HOST, RELEASES_URL_PATH" >&2
  exit 1
fi

BASE_URL="https://${DEPLOY_HOST}/${RELEASES_URL_PATH#/}"
BASE_URL="${BASE_URL%/}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TEMPLATE_FILE="$SCRIPT_DIR/../homebrew/reflections-bin.rb"

ASKPASS_FILE=$(mktemp)
CLONE_DIR=$(mktemp -d)

trap 'rm -f "$ASKPASS_FILE"; rm -rf "$CLONE_DIR"' EXIT

printf '#!/usr/bin/env bash\necho "${HOMEBREW_TAP_TOKEN}"\n' > "$ASKPASS_FILE"
chmod +x "$ASKPASS_FILE"
export GIT_ASKPASS="$ASKPASS_FILE"

git clone "https://x-access-token@github.com/Fizzizist/homebrew-tap.git" "$CLONE_DIR"

mkdir -p "$CLONE_DIR/Formula"
FORMULA_FILE="$CLONE_DIR/Formula/reflections-bin.rb"

if [[ ! -f "$FORMULA_FILE" ]]; then
  cp "$TEMPLATE_FILE" "$FORMULA_FILE"
fi

sed -i.bak "s/^  version .*/  version \"$VERSION\"/" "$FORMULA_FILE"
sed -i.bak "s|^  url .*|  url \"${BASE_URL}/reflections-v#{version}-aarch64-apple-darwin.tar.gz\"|" "$FORMULA_FILE"
sed -i.bak "s/^  sha256 .*/  sha256 \"$SHA256\"/" "$FORMULA_FILE"
rm -f "${FORMULA_FILE}.bak"

cd "$CLONE_DIR"
git config user.name "Release Bot"
git config user.email "release@reflections.local"
git add Formula/reflections-bin.rb
if [[ -n $(git status --porcelain) ]]; then
  git commit -m "Update reflections-bin to v${VERSION}"
  git push origin HEAD:main
else
  echo "Formula already up to date — nothing to commit"
fi
