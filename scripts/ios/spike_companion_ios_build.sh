#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
APP_DIR="$ROOT_DIR/apps/ios/BorgCompanionSpike"

cd "$APP_DIR"
set +e
xcodebuild -scheme BorgCompanionCore -destination 'generic/platform=iOS Simulator' build
STATUS=$?
set -e

if [[ $STATUS -ne 0 ]]; then
  echo
  echo "hint: iOS Simulator destination is unavailable on this host." >&2
  echo "hint: install iOS platform components in Xcode > Settings > Components and create a simulator runtime/device." >&2
  exit $STATUS
fi
