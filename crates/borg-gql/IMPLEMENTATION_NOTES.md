# borg-gql implementation notes

- scope: standalone crate only (`crates/borg-gql`), no integration into `borg-api` yet.
- schema is code-first via `async-graphql` and targets entities from `docs/rfds/RFD0023-borg-gql.md`.
- URI scalar wraps `borg_core::Uri`.
- `JsonValue` scalar remains only on transitional/legacy JSON DB columns.
- taskgraph and memory are wired as typed resolvers over existing store APIs.
- runtime wrappers (`runActorChat`, `runPortHttp`) are placeholders returning typed errors until runtime integration.
- `build.rs` now generates a static `schema.graphql` snapshot during crate build so frontend codegen/inspection can consume a deterministic SDL artifact.
- `SCHEMA_USAGE.md` now includes concrete usage notes + examples for all entity domains and mutation families.

TODO while stabilizing compile/tests:
- fix macro/typing issues from large schema file and prune dead code.
- keep mutation input for `appendSessionMessage` typed-only path consistent.
- ensure Node interface works with all object ID fields.
- harden task status test to avoid auth edge cases if needed.
