# RFD0025 - Frontend Architecture Reset: GraphQL-First Modular Web App

- Feature Name: `frontend_architecture_reset_graphql_first`
- Start Date: `2026-03-04`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

This RFD proposes a clean-break frontend architecture for Borg’s TypeScript/React codebase.  
We keep `@borg/ui` and `@borg/i18n`, move to an app-plus-packages workspace model, adopt GraphQL as the main frontend API contract (aligned with `RFD0023`), and simplify package internals so features own `graphql + model + ui` together with minimal boilerplate.

This is a pre-deployment architecture reset. We do not preserve backward compatibility in frontend structure or API client usage.

## Motivation
[motivation]: #motivation

Current frontend friction is structural:

1. Core files are too large and blend routing, side effects, and UI rendering:
   1. `packages/borg-api/src/index.ts` (~1972 LOC)
   2. `packages/borg-onboard/src/OnboardApp.tsx` (~1030 LOC)
   3. `packages/borg-app/src/DashboardApp.tsx` (~870 LOC)
2. Route resolution is manual instead of router-native.
3. There is no TypeScript project reference graph across frontend packages.
4. App shell imports package internals through root Vite aliases, weakening boundaries.
5. API integration is broad and ad hoc in one client module, reducing feature ownership.
6. Feature behavior tests are sparse outside `@borg/ui`.

These issues reduce speed and consistency for onboarding and dashboard UX work.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### Architecture decision

Borg frontend should be a modular monolith, not runtime microfrontends and not islands-based rendering.

Decision:

1. One main web app shell.
2. Many internal feature packages with explicit boundaries.
3. One typed API contract via GraphQL schema + generated client types.

### Why this decision (expert patterns)

Public lessons from Spotify and Linear suggest:

1. Moving from highly fragmented client views to shared state/component architecture improves speed and consistency.
2. Plugin/slice modularity inside one app shell scales better for product coherence than runtime-fragmented UIs.
3. Strong client data contracts and local state models are critical for real-time app quality.

Inference note:

1. Spotify and Linear do not publish every internal detail.
2. We use only public engineering material to infer patterns relevant to Borg.

### Why not microfrontends now

Microfrontends are useful for many independently shipping teams.  
They also add runtime composition overhead, cross-app state complexity, shared dependency/version drift, and design consistency risk.

Borg currently benefits more from one cohesive app with strict internal package boundaries.

### Why not islands for primary app shell

Islands architecture is excellent for mostly static, content-heavy pages with small interactive regions.  
Borg is primarily an interaction-heavy product shell (chat, control panes, observability views), so full islands-first composition is not the best default.

### Target workspace shape

```text
apps/
  borg-admin/                           # main web runtime
packages/
  borg-ui/                              # primitives + tokens + shared UI language
  borg-i18n/                            # catalogs + translation helpers
  borg-graphql-client/                  # generated schema types + operation documents
  borg-onboarding/
  borg-dashboard-control/
  borg-dashboard-observability/
  borg-devmode/
  borg-test-utils/
  borg-tsconfig/
```

### App model

Inside app/features we use explicit layers:

1. `app` (providers, router, shell composition)
2. `pages` (route modules only)
3. `features` (user workflows with colocated graphql/model/ui)
4. `shared` (ui, graphql client, utilities)

```mermaid
flowchart TD
  A[Route change] --> B[apps/borg-admin router]
  B --> C[Lazy page module]
  C --> D[Feature package]
  D --> E[Feature graphql operation]
  E --> F[/graphql]
```

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

## 1. Non-negotiable constraints

1. Frontend structure is a clean break; no compatibility layer for old package wiring.
2. `borg-cli` remains the only binary crate.
3. Frontend uses GraphQL as the primary typed integration contract.
4. Package boundaries are enforced through package `exports` and lint checks.

## 1.1 Expert pattern mapping (what we copy)

1. Spotify web-player rewrite lesson:
   1. Avoid fragmented iframe-style view architecture for Borg web shell.
   2. Prefer shared state flow and shared component system.
2. Spotify Backstage lesson:
   1. Keep a single app instance with plugin-style modular packages.
   2. Provide shared utility APIs and route indirection rather than hard-coding cross-feature paths.
3. Linear sync architecture lesson:
   1. Treat the client data model as first-class (typed local graph/cache + incremental updates).
   2. Keep transport contracts strict and machine-validated.
4. Linear multi-region lesson:
   1. Keep frontend integration endpoints stable while backend complexity evolves behind a gateway/facade.

## 2. Workspace boundaries

`package.json` workspaces include:

1. `apps/*`
2. `packages/*`

Rules:

1. `apps/borg-admin` is the only runtime entrypoint.
2. Packages are importable only via published entrypoints.
3. No deep imports into sibling package internals.

## 3. Routing architecture

Replace manual pathname parsing with router-native modules:

1. `apps/borg-admin/src/app/router.tsx` owns route tree.
2. Route branches are lazy loaded.
3. URL entity parsing moves to route params/loaders.
4. Feature packages expose route-ready components and handlers.

## 4. GraphQL-first API architecture

GraphQL frontend contract:

1. Backend exposes schema via `/graphql` (see `RFD0023`).
2. Frontend generates typed artifacts from schema and operation documents.
3. Feature packages own their own operation documents/fragments next to feature logic.

Client package shape:

```text
packages/borg-graphql-client/
  schema.graphql
  src/generated/types.ts
  src/generated/operations.ts
  src/runtime/client.ts
  src/runtime/cache.ts
  src/index.ts
```

Requirements:

1. All feature data reads/writes go through typed GraphQL operations.
2. No hand-written `any` response parsing.
3. Operation validation and type generation run in CI.
4. GraphQL validation errors fail build/test workflows before runtime.

## 5. Simplified feature package contract

Feature package contract:

```text
src/
  index.ts
  model/
  graphql/
  ui/
  lib/
  __tests__/
```

Rules:

1. `graphql` contains the feature’s queries/mutations/fragments and typed wrappers.
2. `model` contains state machine/hooks/actions for that feature only.
3. `ui` renders feature UI and depends on `model` and `@borg/ui`.
4. `lib` contains small local helpers; no cross-feature business logic here.
5. `index.ts` is the only cross-package import surface.
6. Feature state transitions are unit-tested.
7. We do not require separate domain/entity packages unless proven necessary later.

## 6. UI consistency model

`@borg/ui` remains source of truth for primitives and visual tokens.

Add curated, reusable experience blocks:

1. chat thread variants
2. async pending/error panels
3. setup completion cards
4. form-in-chat interaction blocks

This avoids repeatedly re-implementing behavior and styling in each feature package.

## 7. TypeScript and build graph

Add TypeScript project references:

1. root `tsconfig.build.json` with references to app + packages
2. per-package `tsconfig.json` with `composite: true`
3. `typecheck`: `tsc -b tsconfig.build.json`

Add task graph runner (recommended: Turborepo):

1. cacheable `build`, `test`, `typecheck`, `lint`
2. affected-task execution in CI

## 8. `tsgo` adoption

Use `tsgo` as a performance lane with explicit guardrails:

1. `typecheck:fast`: `tsgo -b tsconfig.build.json`
2. Keep `typecheck` (`tsc -b`) as release truth until parity confidence is high
3. Track diagnostics drift and runtime in CI telemetry

Status note:

1. `tsgo` is still a native preview effort.
2. Recent status indicates meaningful progress (including build/project references support), but full parity and ecosystem fit still require validation in Borg’s workspace.

## 9. Implementation order (clean break)

1. Create `apps/borg-admin` shell and router, remove legacy path-switch logic.
2. Create `borg-graphql-client` with schema, generated types, and runtime client.
3. Split `@borg/api` responsibilities into GraphQL operation ownership per feature packages.
4. Extract onboarding, control, observability, and devmode into simplified feature packages (`graphql + model + ui`).
5. Rebuild page composition through route modules.
6. Enforce boundary/lint rules and delete legacy wiring.

## 10. Acceptance criteria

Architecture:

1. No core orchestration file exceeds 400 LOC.
2. No deep sibling imports across packages.
3. Router no longer manually parses paths for primary navigation.
4. Workspace has functioning `tsc -b` project graph.
5. Feature packages follow the simplified colocated structure.

GraphQL:

1. Frontend data integration is GraphQL-first.
2. All operations are type-generated from schema.
3. Invalid operation/schema drift fails CI.
4. Feature packages consume generated typed operations only.

Velocity/quality:

1. New onboarding/dashboard UX changes usually touch <= 5 files.
2. Every feature package has behavior tests for core state transitions.
3. Shared chat/pending/error UX primitives are reused, not duplicated.
4. `build:web`, `typecheck`, and test suites pass on CI.

## Drawbacks
[drawbacks]: #drawbacks

1. This reset is disruptive and requires concentrated refactor effort.
2. GraphQL codegen/tooling adds workflow and CI complexity.
3. Strict boundaries require discipline and occasional upfront boilerplate.
4. `tsgo` preview behavior may occasionally diverge from `tsc`.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

### Why this design

1. It solves current coupling and inconsistency directly.
2. It matches patterns used by high-scale product teams: modular internals with unified app experience.
3. It maximizes type safety front-to-back through GraphQL schema contracts and generated types.

### Alternatives considered

Alternative 1: runtime microfrontends now.

1. Rejected for current Borg stage due composition complexity and consistency risk.

Alternative 2: islands-first app shell.

1. Rejected for interaction-heavy dashboard/chat product needs.

Alternative 3: keep REST-first frontend client.

1. Rejected because GraphQL provides stronger schema-level typing and operation-level compile-time guarantees for complex entity relationships.

## Prior art
[prior-art]: #prior-art

Spotify lessons:

1. Spotify’s web-player rewrite described moving away from iframe-segmented views toward a shared React/Redux app, addressing state and consistency pain.
2. Spotify’s Backstage ecosystem emphasizes plugin modularity and shared platform primitives over ad hoc page-level silos.
3. Spotify’s larger app architecture work highlights explicit platform boundaries and reusable integration APIs.

Linear lessons:

1. Linear’s public sync architecture material emphasizes client-local state plus incremental sync packets for responsive UX.
2. Linear’s multi-region architecture keeps complexity behind stable frontend entrypoints (global auth/routing facade and proxy strategy).
3. Linear desktop packaging shows one core app surfaced on web and desktop, reinforcing a unified product shell strategy.

Ecosystem lessons:

1. GraphQL schema + validation + codegen provides strong type contracts from backend to frontend.
2. Microfrontends are valuable in specific org/team topologies, not universally.
3. Islands are a great fit for content pages, less so for deeply interactive app shells.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

1. Which GraphQL client runtime should Borg standardize on (urql, Apollo, or minimal custom client + typed documents)?
2. Do we model local-first optimistic state primarily in client cache, feature model layer, or both?
3. Should devmode and onboarding remain in one app shell or split into separate app entrypoints later?

## Future possibilities
[future-possibilities]: #future-possibilities

1. Persisted GraphQL queries and operation allowlists.
2. Unified schema registry and automated breaking-change detection.
3. Visual regression coverage for critical onboarding/control flows.
4. Full `tsgo` promotion after parity confidence and tooling maturity.

## Research references

1. TypeScript project references: https://www.typescriptlang.org/docs/handbook/project-references
2. React Router `createBrowserRouter`: https://reactrouter.com/api/data-routers/createBrowserRouter/
3. Turborepo structure guidance: https://turborepo.dev/docs/crafting-your-repository/structuring-a-repository
4. Bun workspaces: https://bun.sh/docs/pm/workspaces
5. Bun isolated installs: https://bun.sh/docs/install/isolated
6. GraphQL schema docs: https://graphql.org/learn/schema/
7. GraphQL validation docs: https://graphql.org/learn/validation/
8. GraphQL serving over HTTP: https://graphql.org/learn/serving-over-http/
9. GraphQL Code Generator TypedDocumentNode: https://the-guild.dev/graphql/codegen/plugins/typescript/typed-document-node
10. TypeScript native preview announcement: https://devblogs.microsoft.com/typescript/announcing-typescript-native-previews/
11. TypeScript native repo/status: https://github.com/microsoft/typescript-go
12. Spotify web player rewrite: https://engineering.atspotify.com/2019/03/building-spotifys-new-web-player/
13. Spotify desktop architecture evolution: https://engineering.atspotify.com/2021/09/building-spotifys-new-desktop-app-and-web-player/
14. Spotify Backstage release architecture (part 2): https://engineering.atspotify.com/2026/02/how-we-release-the-spotify-app-part-2/
15. Backstage architecture overview: https://backstage.io/docs/overview/architecture-overview
16. Backstage app structure: https://backstage.spotify.com/learn/standing-up-backstage/3-app-structure/
17. Linear sync engine notes: https://linear.app/now/scaling-the-linear-sync-engine
18. Linear multi-region architecture: https://linear.app/now/how-we-built-multi-region-support-for-linear
19. Linear incident postmortem (sync/client details): https://linear.app/blog/post-mortem-public-api-and-websocket-issues
20. Linear desktop app changelog: https://linear.app/changelog/2019-04-25-linear-desktop-app
21. Micro-frontends article (Martin Fowler): https://martinfowler.com/articles/micro-frontends.html
22. Module Federation concepts: https://webpack.js.org/concepts/module-federation/
23. Astro islands architecture: https://docs.astro.build/en/concepts/islands/
