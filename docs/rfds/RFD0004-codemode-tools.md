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

### Data model

The data model is centered on four tables. The `apps` table stores integration definitions (`app_id`, `name`, `slug`, `description`, `status`, timestamps). The `app_connections` table stores connectivity and auth/config context for an app at user or workspace scope (`connection_id`, `app_id`, optional `user_id` and `workspace_id`, `auth_kind`, `auth_ref_json`, `config_json`, `status`, timestamps). The `capabilities` table stores user-facing operations exposed by apps (`capability_id`, `app_id`, `name`, `slug`, `description`, input/output JSON schemas, `execution_mode`, `execution_spec_json`, `enabled`, timestamps). The `execution_spec_json` payload differs by mode: builtin mode references a handler identifier and mapping config, codemode provides a prompt/spec template plus package and env hints, and shell mode provides command template and sandbox constraints. Finally, `tool_calls` is the execution audit table and captures internal invocation traces (`tool_call_id`, session/task/turn linkage, optional app/capability linkage, tool name, invocation mode, input/output payloads, status/error, timestamps, and duration).

### Internal tools (non-product)

Built-in runtime tools remain first-class for orchestration. In practice this includes the CodeMode family for package discovery, types/examples retrieval, and code execution, along with Shell, Cron, Task, and Memory primitives. These are implementation details that capabilities map to; they are not the product abstraction shown to users.

### Capability execution contract

Given `(app_id, capability_id, input)`, runtime validates input against `input_schema_json`, resolves connection and auth/config context from `app_connections` and secret/account references, dispatches according to `execution_mode`, and then validates output against `output_schema_json` (best-effort in the initial phase). Each internal execution step is persisted in `tool_calls`, and a normalized result is returned to the agent.

### CodeMode role

CodeMode is the primary path for long-tail integrations where no dedicated builtin exists.

For capability execution, CodeMode follows a predictable pattern: discover and select packages, inspect documentation/types/examples, synthesize code from capability spec and input schema, execute with scoped env/network/filesystem permissions, and return a structured JSON result.

## Drawbacks
[drawbacks]: #drawbacks

- More control-plane entities (`apps`, `capabilities`, `connections`) than a single tool table.
- Requires strong schema discipline for consistent capability behavior.
- Dynamic CodeMode-backed capabilities can be less predictable than dedicated builtins.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

### Alternative A: Keep "Tools" as user-facing concept

This approach offers a simpler migration from current wording, but it preserves ambiguity between product concepts and runtime internals. It is rejected.

### Alternative B: Only builtin integrations

This approach offers maximal control and reliability, but it reduces extensibility and slows delivery of new providers. It is rejected.

### Chosen approach

The chosen direction is to keep `Apps expose Capabilities` as the product model, use internal tool orchestration (primarily CodeMode for long-tail providers) as the runtime model, and prioritize observability through `tool_calls`.

## Prior art
[prior-art]: #prior-art

This proposal draws from integration platforms that expose provider-specific actions under connected apps, from workflow systems that model capability catalogs explicitly, and from model-driven execution loops that retrieve packages/docs/types before generating and running code.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

Open questions remain around output schema strictness in v0 (`warn` versus `hard-fail`), scope rules for `app_connections` (user, workspace, or both), minimum redaction requirements for `tool_calls` payload fields, and ranking behavior when both builtin and CodeMode-backed capability implementations are available.

## Future possibilities
[future-possibilities]: #future-possibilities

Future work can add a capability policy engine, capability composition graphs for reusable workflows, promotion pipelines that turn successful CodeMode executions into stable builtins, and user-installable app adapters that still satisfy the same capability contract.
