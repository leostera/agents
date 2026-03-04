#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
APP_DIR="$ROOT_DIR/apps/ios/BorgCompanionSpike"
FIXTURE_DEFAULT="$APP_DIR/fixtures/sample_push_payload.json"

BUNDLE_ID="${BORG_COMPANION_BUNDLE_ID:-com.borg.companion.spike}"
PAYLOAD_PATH="${1:-$FIXTURE_DEFAULT}"
SIM_UDID="${BORG_SIMULATOR_UDID:-}"

if [[ ! -f "$PAYLOAD_PATH" ]]; then
  echo "error: payload not found: $PAYLOAD_PATH" >&2
  exit 1
fi

if [[ -z "$SIM_UDID" ]]; then
  SIM_UDID="$(xcrun simctl list devices booted --json | awk -F '"' '/"udid"/ { print $4; exit }')"
fi

if [[ -z "$SIM_UDID" ]]; then
  echo "error: no booted iOS simulator found" >&2
  echo "hint: open Simulator.app, boot a device, then re-run" >&2
  echo "hint: you can set BORG_SIMULATOR_UDID explicitly" >&2
  exit 2
fi

xcrun simctl push "$SIM_UDID" "$BUNDLE_ID" "$PAYLOAD_PATH"
echo "pushed payload to simulator=$SIM_UDID bundle_id=$BUNDLE_ID"
