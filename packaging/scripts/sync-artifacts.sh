#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <tarball1> [tarball2] ..." >&2
  exit 1
fi

if [[ -z "${DEPLOY_SSH_KEY:-}" || -z "${DEPLOY_HOST:-}" || -z "${DEPLOY_USER:-}" || -z "${DEPLOY_PATH:-}" ]]; then
  echo "Missing required env vars: DEPLOY_SSH_KEY, DEPLOY_HOST, DEPLOY_USER, DEPLOY_PATH" >&2
  exit 1
fi

SSH_KEY_FILE=$(mktemp)
KNOWN_HOSTS_FILE=$(mktemp)
trap 'rm -f "$SSH_KEY_FILE" "$KNOWN_HOSTS_FILE"' EXIT

echo "$DEPLOY_SSH_KEY" > "$SSH_KEY_FILE"
chmod 600 "$SSH_KEY_FILE"

ssh-keyscan -H "$DEPLOY_HOST" >> "$KNOWN_HOSTS_FILE" 2>/dev/null

for tarball in "$@"; do
  if [[ ! -f "$tarball" ]]; then
    echo "File not found: $tarball" >&2
    exit 1
  fi
  scp -i "$SSH_KEY_FILE" -o UserKnownHostsFile="$KNOWN_HOSTS_FILE" "$tarball" "${DEPLOY_USER}@${DEPLOY_HOST}:${DEPLOY_PATH}/"
done
