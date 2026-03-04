#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
APP_DIR="$ROOT_DIR/apps/ios/BorgCompanionSpike"
FIXTURE="$APP_DIR/fixtures/sample_push_payload.json"

cd "$APP_DIR"
swift run BorgCompanionPushFixture "$FIXTURE"
