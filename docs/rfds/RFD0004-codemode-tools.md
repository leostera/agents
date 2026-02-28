# RFD0004 - Apps Expose Capabilities, Internal Tools Power Execution

- Feature Name: `apps_capabilities_execution_runtime`
- Start Date: `2026-02-28`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

We propose an tooling model to make Borg easily extensible while maintaing a
small vetted core, introducing *Apps* to represent 3rdparty integrations, and
**Capabilities** to describe things that apps can do.

We reserve the use of the _tool_ concept for internals tools provided by Borg,
like the `CodeMode.executeCode` tool. This is closer to "MCP Tool".

## Problem statement

In the current model for borg, tools are hardcoded and provied by crates like
borg-ltm and borg-rt. Borg LTM gives us access to `stateFacts` and
`searchMemory`, and Borg RT gives us tools like `executeCode`.

This means that an agent cannot choose what tools it picks, it is given these
tools and must make do with them.

For many use-cases this is fine, but we found it quite limiting when more
complex interactions with external systems appeared: downloading torrents,
accessing calendars, etc.

Suppose a user wanted to download a torrent. Today, this can only be done by
letting the LLM hallucinate the right JavaScript code to do it, and hopefully it will
succeed in making this code: 

1. find the torrent on a search engine, 
2. parse results and acquire the magnet link, and
3. building a small torrent reading engine to download it. 

Another example is trying to list events in a private Google Calendar. To do
this the LLM needs to create code that will:

1. use Google Calendar's API over HTTP, including figuring out authorization
2. finding the endpoint to fetch an iCal,
3. fetch and parse the iCal, and
4. extract the events 

This is most problematic, because today it relies on the LLM being able to
hallucinate the right APIs, parsers for common formats, even entire protocol
implementations.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### Mental model

An App is an external system Borg can connect to, like uTorrent, SerpAPI, or
Google Calendar. Each App exposes one or more Capabilities, which are the
actions users can invoke, such as `uTorrent / Add Torrent` or `SerpAPI / Search
Google`. 

Both Apps and Capabilities are _data_ in our system, backed by tables. Apps
include information like what secrets they require, any docs about them. For
example, the `uTorrent` app would look like this:

```json
{
  id: "borg:app:<uuid>",
  name: "uTorrent",
  description: "Connect to uTorrent to download torrents",
  secrets: [
     { name: "UTORRENT_API" },
     ...
  ],
  capabilities: [
    {
      name: "Add Torrent",
      hint: "Use this tool to add torrents to an existing utorrent instance for downloading",
      mode: "codemode" (could be "shell" or "built-in"),
      secrets: ["UTORRENT_API"],
      instructions: To call this API, use the `npm:@ctrl/utorrent` package.

Example:

<code>
    import {Utorrent} from "npm:@ctrl/utorrent";
    
    async () => {
        const client = new Utorrent({
            baseUrl: Borg.env.get("UTORRENT_API"),
            path: '/gui/',
            password: 'admin',
        });

        const res = await client.addTorrentFromUrl("magnet-link");
        console.log(res);
    }
</code>

"
    },
    ...
  ]
}
```

### Capability discovery and execution flow

To discover capabilities based on intent, the Agent can use a new MCP Tool:
`findCapability(query) -> [ (App, Capability) ]`. Here's an example
conversation, including tool calls:


```text
> me: download the latest minions movie please
> agent: I can do that for you!
  < tool call(findCapability): "i need to find a torrent for a movie"
  > tool resp(findCapability): [
      "SerpAPI / Search Web",
    ]

  < tool call(SerpAPI / Search Web): { query: "latest minions movie torrent" }
  > tool resp(SerpAPI / Search Web): { links: [ ... ] }

> agent: I found 3 links for "Minions: The Rise of Gru", which one should I get?
> me: webrip please

  < tool call(CodeMode): `fetch page, use DOMParser, extract magnet link`
  > tool resp(CodeMode): <magnet link>

  < tool call(uTorrent / Add Torrent): { magnet_or_torrent_url: "...", save_path: "..." }
  > tool resp(uTorrent / Add Torrent): { hash: "...", accepted: true }
  
  < tool call(Memory.stateFacts): [
     borg:torrent:<hash> magnetLink <magnet-link>;
     borg:movie:<uuid> name "Minions: The Rise of Gru";
     borg:movie:<uuid> downloadedFrom borg:torrent:<hash>
    ]

> agent: we are downloading Minions: The Rise of Gru!
> me: excellent! what's the status?

> agent:
  < tool call(uTorrent / Get Torrent Status): { hash: "..." }
  > tool resp(uTorrent / Get Torrent Status): { progress: 0.37, state: "downloading" }
```

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

At runtime, this proposal introduces a capability registry and a dispatcher that binds each capability to one execution mode. The key point is that capability definitions are data, while execution remains in existing Borg runtime primitives. This means we can add new capability definitions quickly without adding new crates or binaries for each provider.

### Data model

The model uses five core tables and keeps each one narrow in responsibility.

The `apps` table describes integration surfaces such as uTorrent, SerpAPI, and Google Calendar. It stores stable identity and display data (`app_id`, `name`, `slug`, `description`) plus lifecycle status and timestamps.

The `app_secrets` table stores secret material for each app. A practical shape is `app_id`, `secret_id`, `hint`, `key`, and `value`, where `value` is encrypted at rest. Runtime decrypts `value` only when invoking a capability that requests that `secret_id`.

The `app_connections` table stores connection context for a specific app in user or workspace scope. This includes whether the connection uses OAuth, API keys, or local service endpoints, plus non-secret configuration such as host, port, and default paths.

The `capabilities` table stores the user-facing operations. Each row is bound to one app and defines its input/output contracts using JSON schema. The row also defines the execution mode (`builtin`, `codemode`, or `shell`) and carries an `execution_spec_json` payload that tells runtime how to execute that capability. Capability specs reference secrets by `secret_id`, not by raw key/value. In builtin mode, the spec points to a vetted internal handler. In codemode, it carries generation and runtime hints such as packages, required secret IDs and env names, and output expectations. In shell mode, it carries command template and sandbox constraints.

The `tool_calls` table is the execution trace log. It captures every internal invocation that occurs while fulfilling a capability call, including tool name, normalized input/output payloads, timing, status, and optional app/capability linkage. This table is the observability backbone for debugging and replay, and it is designed to work before any policy engine exists.

### Dispatch and execution

When a capability is invoked, runtime follows one deterministic sequence:

1. Validate request payload against capability input schema.
2. Resolve app connection context and secret references.
3. Dispatch to the configured execution mode.
4. Validate or normalize the response against capability output schema.
5. Persist trace events to `tool_calls`.

The execution mode defines where logic lives, not what the user sees. Builtin mode uses curated handlers in Borg code. Codemode uses generated JavaScript in the existing execution environment and is the primary path for long-tail provider work. Shell mode is an explicit fallback for CLI-oriented or host-local operations that do not fit the first two modes cleanly.

### Internal tools as execution substrate

Internal tools remain first-class runtime components. In practice, the substrate is `CodeMode.*` for package discovery, type/example retrieval, code generation, and code execution; `Shell.*` for bounded command execution; and `Task.*`, `Memory.*`, and `Cron.*` for stateful orchestration. Capabilities can call one or several of these tools, but the product abstraction stays at `App / Capability`.

### CodeMode contract in this model

CodeMode needs to reliably convert capability intent into executable code while preserving structure in outputs. In this model, codemode execution should consistently perform package selection, docs/type/example retrieval, snippet synthesis, constrained execution, and structured return payloads. This is exactly what allows Borg to add capabilities quickly without waiting on new hardcoded handlers.

### Drawbacks
[drawbacks]: #drawbacks

The tradeoff is operational complexity. A data-driven capability surface is more flexible, but it demands strict schema quality, clear spec conventions, and careful runtime diagnostics. CodeMode-backed capabilities are also less deterministic than builtin handlers, which means teams need strong traces and good failure reporting to keep behavior understandable.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

Keeping user-facing "tools" as the primary abstraction was considered because it would be a lighter migration from current wording. We reject it because it keeps product and runtime concepts entangled, which is one of the things blocking fast extension work.

A builtin-only approach was also considered because it provides the strongest control and predictability. We reject it because it slows capability delivery and forces core-code changes for every provider-specific workflow.

The chosen direction keeps `Apps expose Capabilities` as the product model and uses internal tools as execution infrastructure. This gives Borg a small vetted core while still letting new capability definitions ship quickly.

## Prior art
[prior-art]: #prior-art

The model follows established integration platforms where providers expose action catalogs and operators connect accounts separately from execution. It also aligns with modern agent systems that use code execution loops with package/docs/type retrieval to implement long-tail integrations without hardcoding every operation. A relevant example is Cloudflare's Code Mode write-up, which demonstrates the same general pattern of using code execution as an extension path when built-in surfaces are too narrow.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

The main open questions are about operational boundaries and lifecycle, not direction. We still need to decide how strict output schema enforcement should be in v0, whether app connections should default to user scope or workspace scope, what minimum redaction rules apply to trace payloads in `tool_calls`, and how ranking should behave when both builtin and codemode implementations can satisfy the same capability. We also need to settle capability versioning and compatibility rules, retry/timeout defaults per execution mode, and how secret resolution behaves when both app-level and connection-level values are present.

## Future possibilities
[future-possibilities]: #future-possibilities

Once this model is running in production, we can layer capability grants for agents and policy controls on top of it. That includes explicit allowlists of which agents can invoke which capabilities, allow/deny controls, and quota rules per connected app. We also expect user-installed adapters that satisfy the same capability contract without changing the core product language.
