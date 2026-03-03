# RFD0016 - BorgFS and Audio Messages

- Feature Name: `borgfs_audio_messages`
- Start Date: `2026-03-03`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

This RFD introduces:

1. `borg-fs`: a runtime file storage abstraction with pluggable backends
   (`local` first, `s3` later).
2. A new `BorgInput` audio variant for actor turns.
3. A v0 audio processing pipeline:
   - ports ingest audio
   - audio is persisted in `borg-fs`
   - actor receives a `BorgInput::Audio` message by `file_id`
   - actor transcribes audio and continues processing as text
   - session history stores one combined user message containing
     `file_id + transcript`

Core principle:

1. Audio is an input modality.
2. Text remains the reasoning substrate inside session context.

## Motivation
[motivation]: #motivation

Borg currently treats session turns as text-first. Ports such as Telegram,
Discord, and future HTTP uploads need native audio handling without coupling
runtime logic to one storage backend.

We need:

1. Durable audio storage independent from actor mailbox rows.
2. A backend-agnostic storage contract so Borg can run locally or as a
   hosted service.
3. Clean attribution and replayability in session history.
4. A minimal v0 path that works now, while enabling richer media workflows.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### Mental model

1. Ports receive audio bytes.
2. Ports persist bytes to `borg-fs`.
3. Ports pass a `file_id` to runtime.
4. Actor transcribes and handles the turn as normal text logic.
5. Session history stores one user message with both audio reference and
   transcript.

```mermaid
flowchart TD
  A[Port receives audio] --> B[borg-fs put]
  B --> C[file_id borg:audio:sha512]
  C --> D[enqueue BorgInput::Audio]
  D --> E[Actor fetches audio from borg-fs]
  E --> F[Transcription capability]
  F --> G[Continue turn as text]
  G --> H[Persist combined user message file_id + transcript]
```

### v0 operator experience

1. Audio turn arrives.
2. User gets assistant reply exactly as a normal text turn.
3. History includes transcript and audio reference.
4. No separate audio timeline UI is required in v0.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

### Goals

This RFD MUST provide:

1. A backend-agnostic file storage layer (`borg-fs`).
2. Audio turn ingestion at port boundaries.
3. Runtime handling of `BorgInput::Audio`.
4. Combined session history persistence for audio user turns.
5. Actor mailbox payloads that reference audio by ID, not raw bytes.

### Non-goals (v0)

This RFD does not provide:

1. Text-to-speech output.
2. Streaming transcription.
3. Rich multi-part media timelines.
4. Full media search indexing beyond metadata.
5. Cross-region replication and archival tiers.

### BorgFS contract

`borg-fs` is a crate-level abstraction used by ports and runtime.

Minimum v0 trait:

1. `put(bytes, metadata) -> FileRecord`
2. `get(file_id) -> FileBytes + FileRecord`
3. `exists(file_id) -> bool`
4. `soft_delete(file_id) -> ()`

Optional later:

1. `list(prefix, filters)`
2. `search(metadata query)`
3. `signed_url(file_id, ttl)`

#### File identity

1. `file_id` format: `borg:audio:<sha512>`
2. Content hash is computed from raw bytes.
3. Duplicate uploads by hash MAY reuse existing storage object.

#### v0 backends

1. `LocalFsBackend` (default):
   - rooted under `~/.borg/files`
2. `S3Backend` (future within this RFD scope):
   - configured bucket + prefix
   - object key derived from `file_id`

### Data model

#### `files`

- `file_id` (pk, URI-like, e.g. `borg:audio:<sha512>`)
- `backend` (`local | s3 | ...`)
- `storage_key`
- `content_type`
- `size_bytes`
- `sha512`
- `owner_uri` (nullable)
- `metadata_json`
- `deleted_at` (nullable; soft delete)
- `created_at`
- `updated_at`

#### Session message payload (combined audio user message)

Stored in `session_messages.payload_json` as one message:

```json
{
  "type": "user_audio",
  "file_id": "borg:audio:<sha512>",
  "mime_type": "audio/m4a",
  "duration_ms": 12345,
  "transcript": "hello world",
  "transcription_provider": "openai",
  "transcription_model": "whisper-1",
  "created_at": "2026-03-03T00:00:00Z"
}
```

### Runtime and port flow

#### 1. Port ingress

Ports that support audio MUST:

1. Accept audio bytes + basic metadata (mime, optional duration).
2. Persist bytes through `borg-fs.put`.
3. Construct runtime input with `file_id`.

#### 2. Borg message input

Add:

1. `BorgInput::Audio { file_id, mime_type, duration_ms, language_hint }`

The `BorgMessage` envelope remains the same (`actor_id`, `user_id`,
`session_id`, `port_context`).

#### 3. Actor handling

On `BorgInput::Audio`:

1. Fetch bytes from `borg-fs.get(file_id)`.
2. Run transcription capability/tool.
3. Persist combined `user_audio` message payload in session history.
4. Continue the turn as standard chat text using `transcript`.

#### 4. Context window behavior

Context manager uses transcript text, not binary audio.
The `file_id` remains available for audit, replay, and UI.

### API boundary changes

v0 does not require one universal upload API shape, but HTTP-capable ports
SHOULD expose a path that accepts audio upload and maps to `BorgInput::Audio`.

The API must never embed raw audio in mailbox/session JSON rows.

### Error handling

Failures should be explicit:

1. Unsupported mime type.
2. Audio too large / too long.
3. Storage write failure.
4. Transcription failure.
5. Missing `file_id` in storage.

v0 policy:

1. No automatic retries beyond existing port/runtime retry behavior.
2. Return a clear user-facing error message in the session.

### Observability and audit

Each audio turn SHOULD log:

1. `file_id`, `size_bytes`, `mime_type`
2. transcription provider/model
3. transcription latency
4. actor/session/message attribution

Do not log raw audio bytes.

## Future work
[future-work]: #future-work

### BorgFS MCP tools

This RFD reserves a future MCP surface for file operations bounded by BorgFS:

1. `BorgFS-ls`
2. `BorgFS-get`
3. `BorgFS-put`
4. `BorgFS-delete` (soft delete only)
5. `BorgFS-search`

Guardrails:

1. URI/key-based access only (no arbitrary host path traversal).
2. Namespace and ownership policy checks.
3. Full audit trail for all mutations and reads.

### Potential extensions

1. Segment-level transcripts with timestamps.
2. Audio preview playback in UI.
3. TTS assistant output.
4. Lifecycle policies (retention, archival, restore).

## Drawbacks
[drawbacks]: #drawbacks

1. Adds a new storage subsystem to runtime complexity.
2. Requires policy decisions around retention and privacy.
3. Increases operational footprint when enabling hosted mode.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

### Alternative 1: store audio bytes directly in `session_messages`

Rejected because:

1. Bloats hot query tables.
2. Couples runtime DB strongly to media payload sizes.
3. Makes backend portability harder.

### Alternative 2: make transcription a port-local concern only

Rejected because:

1. Duplicates logic across ports.
2. Weakens runtime consistency and observability.
3. Prevents unified audio behavior across ingress surfaces.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

1. Exact retention defaults for soft-deleted files.
2. Whether v0 enforces one canonical audio MIME set or configurable provider set.
3. Whether to attach transcript confidence metadata in v0 payload.
