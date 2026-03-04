# RFD0020 - Serve Dashboard from `borg-api` (`borg start`)

- Feature Name: `dashboard_from_borg_api`
- Start Date: `2026-03-03`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

We want `borg start` to bring up a complete local Borg machine, including the admin dashboard, on one address (`http://localhost:8080/` by default). This RFD proposes shipping dashboard assets inside the Borg binary, serving HTML/CSS/JS directly from embedded memory, and materializing selected static media assets (for example images/fonts) into `~/.borg/assets` for `/assets/*` serving.

## Motivation
[motivation]: #motivation

Today the local experience is split:

1. start runtime/API with `borg start`
2. separately run frontend dev server (`bun run dev`)
3. remember which URL is API and which is dashboard

This creates friction exactly where Borg should feel strongest: local, fast, always-on operation.

Desired UX:

- one command to start Borg
- one URL to open
- no accidental API/dashboard mismatch
- clear failure mode if dashboard assets are missing

This aligns with Borg's value proposition:

- local-first
- fast feedback loop
- always-on runtime
- channel-native operations

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### New operator experience

```bash
$ borg start
```

Startup logs include:

```text
Borg API server listening on 127.0.0.1:8080
Open http://127.0.0.1:8080/ for admin dashboard
```

Operator opens `/` and sees the dashboard.

API remains under `/api/*`, ports under `/ports/*`, health under `/health`.

### Dev workflow

For frontend development, contributors may still run `bun run dev` on `localhost:5173`.
This RFD does not remove that workflow. It standardizes that production/local-runtime UX is served by `borg-api`.

### Runtime behavior at `/`

- `/` serves SPA `index.html`.
- HTML/CSS/JS are served directly from embedded memory.
- static media assets (images/fonts/etc) are served from `~/.borg/assets`.
- unknown non-API routes (for client-side routing) return SPA `index.html`.

### Missing assets

If static media extraction/sync fails, behavior must be explicit:

- `borg start` logs a high-signal warning/error with recovery command.
- if configured as strict mode, startup fails loudly.

Recommended recovery message:

```text
Static media assets failed to materialize under ~/.borg/assets.
```

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

## Routing model

`borg-api` router precedence:

1. API and runtime routes first:
   - `/api/*`
   - `/ports/*`
   - `/health`
   - OAuth callback endpoints used by app integrations
2. static file serving for dashboard assets
3. SPA fallback to `index.html` for non-API routes

This avoids accidentally swallowing API routes into the SPA fallback.

## Asset source options

### Option A: serve from filesystem dist directory

- Source path: `packages/borg-app/dist`
- Pros:
  - simple
  - no binary size increase
  - easy to refresh by rebuilding web assets
- Cons:
  - runtime depends on local filesystem state

### Option B: embed frontend assets in Rust binary; serve core assets from memory and sync static media to `~/.borg/assets`

- Pros:
  - fully self-contained runtime binary
  - HTML/CSS/JS do not depend on filesystem extraction
  - predictable runtime path for media assets
  - no missing-dist class of errors in normal startup
  - stable `/assets/*` serving independent of repository layout
- Cons:
  - larger binaries
  - more complex build pipeline coupling
  - requires partial sync/manifest logic for media assets

### Decision for v0

Adopt **Option B** now:

1. embed dashboard assets in the binary
2. serve HTML/CSS/JS directly from embedded memory
3. sync static media assets into `~/.borg/assets` at startup
4. serve `/assets/*` from `~/.borg/assets`

Keep Option A (`BORG_DASHBOARD_DIST`) as a dev override only.

Rationale: better one-command UX and stronger local-first packaging.

## Startup contract

`borg-cli start` should:

1. initialize storage/migrations as today
2. materialize/sync embedded static media assets into `~/.borg/assets`
3. start `borg-api` with in-memory serving for HTML/CSS/JS and filesystem serving for `/assets/*`
4. print explicit dashboard URL in logs/stdout

Config knobs (optional, v0 defaults):

- `BORG_DASHBOARD_ASSETS_DIR` (override extraction/serving path; default `~/.borg/assets`)
- `BORG_DASHBOARD_DIST` (dev override to serve local dist directly)
- `BORG_DASHBOARD_STRICT=1` (fail startup if static media extraction/sync fails)

## Build-time guarantees

Dashboard artifacts (for example the built JS/CSS bundles referenced by `index.html`) must be validated at build time.

If required frontend artifacts are missing, the build must fail immediately rather than deferring failure to runtime.

This keeps release/runtime behavior predictable and avoids shipping a binary that cannot render the admin dashboard.

## HTTP behavior details

- `GET /` -> embedded `index.html` (memory)
- `GET /assets/*` -> files under `~/.borg/assets/assets/*`
- `GET /control/apps` (or other SPA path) -> embedded `index.html` (memory)
- `GET /api/...` -> API handler (never SPA fallback)
- `GET /health` -> health handler

## Asset sync semantics

On each startup:

1. read embedded media-asset manifest (path + hash)
2. compare with `~/.borg/assets/.manifest.json` (if any)
3. write/update only changed files
4. update manifest atomically

This keeps startup fast while ensuring static media assets match the running binary.

## Observability

At startup:

- log dashboard mode (`served_from=embedded_memory+assets_dir` or `served_from=dist_override`)
- log resolved assets path
- log dashboard URL
- log extraction/sync remediation if needed

Per request:

- retain existing HTTP trace logs
- include static-route responses in normal tracing

## Rollout plan

1. Add frontend asset embedding in build pipeline.
2. Add build-time validation that required dashboard artifacts exist; fail build if missing.
3. Add static media asset sync module to materialize selected embedded files into `~/.borg/assets`.
4. Add in-memory serving for HTML/CSS/JS and `/assets/*` filesystem serving in `borg-api`.
5. Add startup messaging in `borg-cli`.
3. Keep `bun run dev` workflow untouched.
4. Add tests:
   - media sync writes expected files to assets dir
   - `/` serves embedded HTML without filesystem dependency
   - SPA fallback works for client routes
   - `/api/*` is not swallowed
   - sync failure emits clear error/warning

## Drawbacks
[drawbacks]: #drawbacks

- Larger binaries.
- Added startup complexity (media-asset sync/manifest management).
- More responsibility inside `borg-api` (serving API + UI).
- Potential confusion during active frontend dev if both 5173 and 8080 are available.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

Alternative 1: keep split servers forever.

- Rejected because it weakens local-first "one command, one URL" ergonomics.

Alternative 2: continue serving only local dist forever.

- Rejected because it weakens packaging and one-command ergonomics.

Alternative 3: reverse proxy from API to Vite dev server.

- Useful in dev only, but not a stable runtime contract.

## Prior art
[prior-art]: #prior-art

- Many local-first systems ship a single process exposing both API and SPA.
- Cloud tooling CLIs commonly print "Open http://..." after startup for operator clarity.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

- Should media-asset sync failure be hard-fail by default, or warning by default?
- Do we want a runtime flag to disable dashboard serving (`--no-dashboard`)?
- Do we want auto-open browser behavior, or keep explicit/manual open only?
- Do we remove stale files on sync, or rely purely on manifest paths?

## Future possibilities
[future-possibilities]: #future-possibilities

- Optional asset compression strategy to reduce binary size.
- Version stamp endpoint exposing API/web build compatibility.
- Health subcheck that reports dashboard-asset availability separately.
