# RFD0012 - Memory Drivers: SQLite Default, PostgreSQL Optional

- Feature Name: `memory_drivers_sqlite_postgres`
- Start Date: `2026-03-02`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

This RFD proposes a pluggable `borg-memory` driver model with `sqlite` as the default local backend and `postgres` as an optional backend for shared/runtime deployments. The memory contract (facts as source-of-truth, derived projections, and `Memory-*` tool behavior) remains unchanged; only storage wiring becomes driver-based.

## Motivation
[motivation]: #motivation

RFD0010 moved memory internals to a single local SQLite `memory.db`, which improves debug velocity and local operability. We now need two operational modes:

1. local-first mode with zero external dependencies (SQLite)
2. shared/multi-process mode where memory can live in managed infrastructure (PostgreSQL)

Without explicit drivers, introducing PostgreSQL risks leaking backend-specific code throughout `MemoryStore`, projection logic, and tooling.

The goal is to preserve one memory behavior contract while allowing backend selection at runtime.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### Mental model

`borg-memory` exposes one logical model:

1. append-only `facts` + retractions
2. projection tables (`entities`, `schemas`, `entity_edges`, search index state)
3. stable `Memory-*` tool contracts

A memory driver implements how those tables and queries are stored and executed.

Default remains SQLite.

```mermaid
flowchart TD
  A[MemoryStore API] --> B[MemoryDriver trait]
  B --> C[SQLiteDriver default]
  B --> D[PostgresDriver optional]
  C --> E[memory.db]
  D --> F[PostgreSQL schema]
```

### Configuration

Proposed runtime config precedence:

1. `BORG_MEMORY_DRIVER` (`sqlite` or `postgres`)
2. persisted runtime config (future control-plane setting)
3. default `sqlite`

Postgres DSN source:

1. `BORG_MEMORY_DATABASE_URL` (required when driver is `postgres`)

Example:

```bash
BORG_MEMORY_DRIVER=postgres
BORG_MEMORY_DATABASE_URL=postgres://user:pass@localhost:5432/borg_memory
```

### Contributor expectation

Most development should continue on SQLite. PostgreSQL is for deployments that need externalized/shared memory state. Tests should run against SQLite by default; PostgreSQL coverage should be additive.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

## Driver interface

Introduce internal traits to isolate backend differences:

1. `FactStoreDriver`
2. `ProjectionDriver` (entities/schemas/edges)
3. `SearchDriver`
4. `MemoryDriver` façade to construct the above for `MemoryStore`

`MemoryStore` depends on traits, not concrete SQLite structs.

Normative invariants across drivers:

1. `state_facts` is atomic per call and returns one generated `tx_id`
2. facts are immutable except retraction marker updates
3. projection replay from facts is deterministic
4. `Memory-search` and `Memory-getEntity` semantics remain backend-independent

## SQLite driver

SQLite remains the default implementation from RFD0010:

1. single `memory.db`
2. `search_fts` via FTS5
3. local pragmas (`WAL`, busy timeout)

## PostgreSQL driver

PostgreSQL mirrors logical schema:

1. `facts`, `projection_queue`, `entities`, `schemas`, `entity_edges`
2. text search implemented with PostgreSQL full-text search (`tsvector`, `GIN`) or trigram fallback
3. migration handled via SQLx migrations under a dedicated driver namespace

Postgres-specific behavior constraints:

1. no backend-specific semantics exposed at tool boundary
2. transaction boundaries align with existing write API contracts
3. projection worker remains idempotent with safe retry on conflict

## Driver selection

At startup:

1. resolve configured driver
2. initialize driver-specific pools/connections
3. run driver migrations
4. start shared projection loop using trait methods

Startup must fail loudly on invalid driver value or missing DSN for PostgreSQL.

## Migration and rollout

Phased rollout:

1. refactor current SQLite code behind traits, no behavior change
2. add PostgreSQL driver with feature gate (`memory-postgres`)
3. add integration tests for driver parity
4. expose runtime configuration and docs

No forced data migration is in scope for initial driver support. Operators can choose fresh Postgres memory or run explicit export/import tooling later.

## Testing strategy

Minimum parity tests required for PostgreSQL:

1. facts write/read/retract behavior
2. projection replay determinism
3. search namespace/kind filters
4. explore/subgraph traversal semantics

SQLite remains the required baseline in CI.

## Drawbacks
[drawbacks]: #drawbacks

1. additional abstraction increases code surface and maintenance cost
2. cross-backend parity testing increases CI complexity
3. PostgreSQL search behavior may differ subtly from SQLite FTS5 scoring

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

Chosen design keeps default simplicity while enabling production flexibility.

Alternatives considered:

1. SQLite-only forever:
   - rejected because it blocks shared externalized memory deployments.
2. Postgres-only:
   - rejected because it degrades local-first onboarding and debug ergonomics.
3. Driverless conditional code paths:
   - rejected because backend concerns would leak across memory code, making behavior harder to reason about.

## Prior art
[prior-art]: #prior-art

1. systems that use SQLite locally and Postgres in hosted deployments
2. repository/driver patterns in storage-heavy Rust services
3. event-log + projection model with backend-specific adapters

## Unresolved questions
[unresolved-questions]: #unresolved-questions

1. exact crate boundaries for driver traits vs concrete implementations
2. whether PostgreSQL support should be compile-time feature gated, runtime-only, or both
3. canonical search ranking behavior definition across SQLite FTS5 and PostgreSQL search
4. timeline for operator export/import tooling between drivers

## Future possibilities
[future-possibilities]: #future-possibilities

1. additional drivers (for example managed cloud KV/vector systems) behind the same contract
2. online dual-write/verification mode to migrate between drivers safely
3. admin diagnostics that report driver health, lag, and projection drift uniformly
