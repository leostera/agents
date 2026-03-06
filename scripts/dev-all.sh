#!/usr/bin/env bash
set -euo pipefail

# Start all local dev servers:
# - Vite app workspace
# - Stage actor playground
# - openborg marketing + platform docs
# - standalone operator docs
# - Storybook
bun run dev:web &
pid_web=$!

bun run --cwd apps/stage dev -- --port 5174 &
pid_stage=$!

bun run dev:www &
pid_www=$!

bun run dev:www-standalone &
pid_www_docs=$!

bun run storybook &
pid_storybook=$!

cleanup() {
  kill "$pid_web" "$pid_stage" "$pid_www" "$pid_www_docs" "$pid_storybook" 2>/dev/null || true
}

trap cleanup EXIT INT TERM

# If any server exits/fails, stop the others and bubble the failure.
wait -n "$pid_web" "$pid_stage" "$pid_www" "$pid_www_docs" "$pid_storybook"
status=$?
cleanup
wait "$pid_web" "$pid_stage" "$pid_www" "$pid_www_docs" "$pid_storybook" 2>/dev/null || true
exit "$status"
