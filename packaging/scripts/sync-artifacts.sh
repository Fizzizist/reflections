#!/usr/bin/env bash
# Uploads release tarball artifacts to peter.vlasveld.info via SCP.
#
# Environment variables:
#   DEPLOY_SSH_KEY  — private SSH key for the deploy host
#   DEPLOY_HOST     — hostname (e.g. peter.vlasveld.info)
#   DEPLOY_USER     — SSH username
#   DEPLOY_PATH     — remote directory path
#
# Arguments:
#   $1 — local tarball path to upload

set -euo pipefail

TARBALL="${1:?Usage: sync-artifacts.sh <tarball-path>}"

if [[ -z "${DEPLOY_SSH_KEY:-}" ]] || [[ -z "${DEPLOY_HOST:-}" ]] || \
   [[ -z "${DEPLOY_USER:-}" ]] || [[ -z "${DEPLOY_PATH:-}" ]]; then
    echo "ERROR: DEPLOY_SSH_KEY, DEPLOY_HOST, DEPLOY_USER, and DEPLOY_PATH must be set" >&2
    exit 1
fi

if [[ ! -f "$TARBALL" ]]; then
    echo "ERROR: tarball not found: $TARBALL" >&2
    exit 1
fi

SSH_KEY_FILE=$(mktemp)
cleanup() {
    rm -f "$SSH_KEY_FILE"
}
trap cleanup EXIT

printf '%s\n' "$DEPLOY_SSH_KEY" > "$SSH_KEY_FILE"
chmod 600 "$SSH_KEY_FILE"

scp -i "$SSH_KEY_FILE" -o StrictHostKeyChecking=no \
    "$TARBALL" \
    "${DEPLOY_USER}@${DEPLOY_HOST}:${DEPLOY_PATH}/"

echo "Uploaded: $(basename "$TARBALL")"