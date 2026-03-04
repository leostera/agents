#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
APP_DIR="$ROOT_DIR/apps/macos/BorgMenuSpike"
BIN="$APP_DIR/.build/debug/BorgMenuSpike"

if [[ ! -x "$BIN" ]]; then
  "$ROOT_DIR/scripts/macos/spike_menu_build.sh"
fi

AUTOTERM="${BORG_MENU_SPIKE_AUTOTERMINATE_SECONDS:-0}"
PHRASE="${BORG_VOICEWAKE_PHRASE:-hey borg}"
SILENCE="${BORG_VOICEWAKE_SILENCE_SECONDS:-1.4}"

BORG_MENU_SPIKE_AUTOTERMINATE_SECONDS="$AUTOTERM" \
  BORG_VOICEWAKE_PHRASE="$PHRASE" \
  BORG_VOICEWAKE_SILENCE_SECONDS="$SILENCE" \
  "$BIN"
