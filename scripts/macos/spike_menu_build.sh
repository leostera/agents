#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
APP_DIR="$ROOT_DIR/apps/macos/BorgMenuSpike"

mkdir -p /tmp/clang-module-cache

cd "$APP_DIR"
CLANG_MODULE_CACHE_PATH=/tmp/clang-module-cache swift build -c debug
echo "built: $APP_DIR/.build/debug/BorgMenuSpike"
