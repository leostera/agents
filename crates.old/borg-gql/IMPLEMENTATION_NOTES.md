# borg-gql implementation notes

- scope: standalone crate only (`crates/borg-gql`), no integration into `borg-api` yet.
- schema is code-first via `async-graphql` and targets entities from `docs/rfds/RFD0023-borg-gql.md`.
- URI scalar wraps `borg_core::Uri`.
- `JsonValue` scalar remains only on transitional/legacy JSON DB columns.
- taskgraph and memory are wired as typed resolvers over existing store APIs.
- runtime wrappers (`runActorChat`, `runPortHttp`) are placeholders returning typed errors until runtime integration.
- `build.rs` now generates a static `schema.graphql` snapshot during crate build so frontend codegen/inspection can consume a deterministic SDL artifact.
- `SCHEMA_USAGE.md` now includes concrete usage notes + examples for all entity domains and mutation families.
- subscriptions are implemented in `SubscriptionRoot`:
  - `sessionChat(sessionId, afterMessageIndex, pollIntervalMs)`
  - `sessionNotifications(sessionId, afterMessageIndex, pollIntervalMs, includeUserMessages)`
- subscription implementation follows practical GraphQL subscription guidance:
  - tail-follow defaults for chat streams (avoid replay unless cursor/index provided)
  - bounded poll interval to protect server resources
  - typed notification payloads (no unstructured JSON envelopes)
  - documentation/examples embedded directly in SDL via GraphQL descriptions
