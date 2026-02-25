Below is a bare-bones but buildable SPEC for Borg: a single-binary, stateless agent orchestration runtime that executes dynamic work graphs, integrates ports (Telegram/email/etc), and persists memory (knowledge graph) while running agent sessions in a sandboxed JS runtime with search + execute (+ create_task) tools.

⸻

Borg — Prototype Spec (v0)

1) Goal

Build a single Rust binary that can be dropped onto a machine (or container / edge-ish environment) and run as an agent orchestration engine.

It:
	•	receives human/system inputs via Ports
	•	converts inputs into Tasks
	•	executes tasks through a Work Graph Engine
	•	runs Agents (LLM loop) in a sandbox with a tiny toolbelt (search, execute, create_task)
	•	persists “what happened” into Memory (knowledge graph)
	•	optionally exposes a Control Plane (dashboard / admin API)

Key constraints
	•	Single binary
	•	Stateless compute (no required local state); state lives in “Universe” backends (queue/memory/config)
	•	Easy container deployment
	•	Efficient at dynamic task/work graphs

⸻

2) Core Concepts

2.1 Universe

A Universe is the external state the binary connects to:
	•	task queue / graph storage (can be in-memory for MVP; durable later)
	•	memory store (knowledge graph)
	•	port configs and secrets
	•	execution policies (limits, sandbox permissions)

Borg instances can come and go; a Universe is what makes work resumable.

2.2 Work Graph

A dynamic graph:
	•	Nodes = Tasks
	•	Edges = dependencies/parent-child relationships
	•	Tasks can spawn more tasks at runtime (subgraphs)

2.3 Task

An immutable-ish unit of work with lifecycle.

Minimal fields:
	•	task_id
	•	universe_id
	•	created_at
	•	status: queued | running | blocked | succeeded | failed | canceled
	•	kind: user_message | agent_action | tool_call | system
	•	payload: JSON (message text, structured command, etc.)
	•	parent_task_id?
	•	depends_on: [task_id]
	•	claimed_by? (worker id)
	•	attempts, last_error?

2.4 Agent Session

A session is a logical thread of work (often rooted at a user message):
	•	maps to a root task + its descendants
	•	maintains conversational state (but stored externally; not in RAM only)

2.5 Ports

Pluggable input/output adapters:
	•	Telegram bot, Email, CLI webhook, etc.
Ports:
	•	ingest external events → produce tasks
	•	emit outputs back to humans (messages, prompts, confirmations)

2.6 Memory (Knowledge Graph)

A graph-ish store for:
	•	entities (movie, torrent, folder, user preference)
	•	relationships (downloaded_from, stored_at, requested_by)
	•	events (task ran, tool executed)
	•	searchable text + structured properties

Must support:
	•	upsert entity
	•	add relation
	•	search (fuzzy / hybrid)
	•	fetch by id

2.7 Agent Runtime

Per task, an agent runs a loop:
	•	reads task + relevant memory context
	•	plans
	•	uses tools:
	•	search(query) → returns available capabilities (tool signatures / actions)
	•	execute(code) → runs sandboxed JS/TS against provided host APIs
	•	create_task(spec) → spawns tasks (graph expansion)

Sandbox: V8 isolate (or equivalent) with constrained host functions.

⸻

3) High-Level Architecture

Single binary with modules:
	1.	Port Manager

	•	loads configured ports
	•	receives events
	•	normalizes → Task::user_message
	•	emits messages back out (status, prompts, results)

	2.	Work Graph Engine

	•	stores tasks + dependencies
	•	scheduler: picks runnable tasks
	•	worker pool: claims tasks, runs them, commits results
	•	supports dynamic task creation

	3.	Agent Executor

	•	“runs the work” for tasks that require agent reasoning
	•	manages agent sessions
	•	calls Memory + tools

	4.	Memory Service

	•	API over the KG backend (embedded SQLite for MVP, pluggable later)
	•	search index (basic FTS for MVP)

	5.	Tool Registry

	•	the “capabilities catalog” that search() queries
	•	each tool has:
	•	name
	•	signature (TS type-ish)
	•	description
	•	permission requirements
	•	implementation target (host function or JS library)

	6.	Sandbox / JS Runtime

	•	executes user/agent-generated JS
	•	exposes host bindings (fetch, filesystem, torrent APIs, etc.) gated by policy

	7.	Control Plane

	•	minimal admin API + optional web UI
	•	inspect tasks, runs, logs, memory entries
	•	manage secrets/config, port settings, permissions

⸻

4) Data Flow Walkthrough (Movie Download)

Scenario

User via Telegram: “Download Minions”

A) Port Subsystem (Telegram)
	1.	Telegram webhook (or long poll) receives message event:
	•	{chat_id, user_id, text="Download Minions", timestamp}
	2.	Port normalizes into a Task:
	•	kind=user_message
	•	payload = { port:"telegram", chat_id, user_id, text }
	•	parent_task_id = null
	3.	Port submits task to Work Graph Engine: enqueue(task)

Output: a queued root task.

⸻

B) Work Graph Engine (Scheduling)
	4.	Scheduler scans for runnable tasks:
	•	no dependencies → runnable
	5.	Worker claims task:
	•	sets status=running, claimed_by=worker-7
	6.	Dispatches to Agent Executor because kind=user_message

Output: task handed to agent runtime.

⸻

C) Agent Runtime (LLM loop)
	7.	Agent Executor loads:
	•	task payload (text, user_id)
	•	session context (if any) by (user_id, port) mapping
	8.	Agent does Memory prefetch:
	•	calls Memory: search("Minions", filters={type:"movie"})
	9.	Two branches:

Branch 1: Found existing
	•	Agent sees Movie{title:"Minions", downloaded:true, stored_at:"/plex/movies/minions.mkv"}
	•	Agent emits response: “Already downloaded. Want me to open it / share link?”

Branch 2: Not found (likely)
10. Agent needs capabilities → calls search("torrent search Minions")

⸻

D) Tool: search(query) → Capability Discovery
	11.	Tool Registry returns matching capabilities, e.g.

	•	torrents.search(query: string) -> Promise<TorrentResult[]>
	•	torrents.download(magnet: string, dest: string) -> Promise<DownloadReceipt>
	•	prefs.get(key: string) -> Promise<string | null>
	•	memory.upsert_entity(entity: Entity) -> Promise<EntityId>
	•	memory.link(from: EntityId, rel: string, to: EntityId) -> Promise<void>

Agent now knows “what can be done” and how to call it.

⸻

E) Agent asks follow-up or acts
	12.	Agent checks preferences:

	•	calls execute(...) to run prefs.get("torrents.dest") (or calls memory directly if exposed)

	13.	If dest missing:

	•	emits prompt via Port: “Where should I save movies? (e.g. /media/plex/movies)”
	•	creates a blocked task waiting for user input OR marks session awaiting input.

⸻

F) Port receives user selection
	14.	User replies: “Save to /media/plex/movies. Use the first result.”
	15.	Port creates a new Task::user_message with:

	•	parent_task_id = original_root_task_id (or session id link)
	•	dependency: can be free, but it semantically resumes the session

Scheduler picks it up; Agent resumes.

⸻

G) Tool: execute(code) → Side effects
	16.	Agent runs JS to:

	•	results = await torrents.search("Minions 1080p")
	•	pick one
	•	receipt = await torrents.download(results[0].magnet, "/media/plex/movies")

Execution happens in the sandbox:
	•	limited CPU/time
	•	limited network targets (policy)
	•	no arbitrary disk unless allowed

⸻

H) Memory writes
	17.	On success, agent persists:

	•	entity: Movie{title:"Minions", year:..., downloaded:true}
	•	entity: Torrent{magnet, hash, source:"PirateBay", meta...}
	•	relation: Movie downloaded_from Torrent
	•	relation: Movie stored_at Location{/media/plex/movies/...}
	•	event: DownloadReceipt{task_id, duration, bytes}

⸻

I) Completion + Output
	18.	Agent emits final message via Port:

	•	“Downloaded Minions to /media/plex/movies/minions.mkv”

	19.	Work Graph Engine marks tasks succeeded, links results, stores logs.

⸻

5) MVP Scope (Prototype)

Must-have (v0)
	•	Single binary borg
	•	In-memory task queue + SQLite-backed persistence (so you can restart and not lose everything)
	•	One port: HTTP webhook port (easier than Telegram first)
	•	optional: Telegram as second
	•	One agent “runner”:
	•	doesn’t need perfect LLM integration for v0; can stub with a simple planner or wire to one model
	•	Tool registry + search(query) returning tool signatures
	•	JS sandbox + execute(code) that can call a couple host functions
	•	Memory store:
	•	entities + relations in SQLite
	•	full-text search for entity labels/notes
	•	Minimal control plane:
	•	GET /tasks, GET /tasks/:id
	•	GET /memory/search?q=...
	•	logs per task

Explicit non-goals for v0
	•	Distributed multi-node scheduling
	•	Sophisticated DAG optimization
	•	Rich UI dashboard
	•	Multi-tenant auth
	•	Perfect graph database (we’ll start with “graph-ish tables”)

⸻

6) Task Graph Engine Details

6.1 Storage model (SQLite MVP)

Tables:
	•	tasks(task_id PK, parent_task_id, status, kind, payload_json, created_at, updated_at, claimed_by, attempts, last_error)
	•	deps(task_id, depends_on_task_id)
	•	task_events(event_id PK, task_id, ts, type, payload_json)
	•	sessions(session_id PK, user_key, port, root_task_id, state_json, updated_at)

6.2 Scheduler rules (simple)

A task is runnable when:
	•	status == queued
	•	all dependencies are succeeded

Claiming:
	•	atomic update: queued → running with claimed_by
	•	heartbeat optional (later)

Retries:
	•	attempts < max_attempts
	•	exponential backoff stored in payload or task_events

⸻

7) Agent Runtime Interface

7.1 Tool API contract (what the agent “sees”)

The agent only sees:
	•	search(query: string) -> Capability[]
	•	execute(code: string) -> ExecutionResult
	•	create_task(task_spec: TaskSpec) -> TaskId

Where Capability includes:
	•	name
	•	signature (TS-like string)
	•	description
	•	examples (optional)

7.2 execute(code) contract
	•	runs in sandbox
	•	returns:
	•	stdout, stderr
	•	result_json
	•	tool_calls (if your sandbox calls host tools)
	•	metrics (time, memory)

Host bindings (MVP):
	•	memory.search(q, filters)
	•	memory.upsert(entity)
	•	memory.link(a, rel, b)
	•	ports.send(port, target, message)
	•	(optional) http.fetch(...)

In v0, keep it tiny. You can fake “torrent download” with a placeholder tool that writes an entity.

⸻

8) Port System (MVP)

Port interface
	•	ingest(event) -> TaskSpec[]
	•	emit(output) -> void

MVP ports:
	1.	HTTP Port

	•	POST /ports/http/inbox accepts {user_key, text, metadata}
	•	returns immediate ack {task_id}

	2.	(Optional next) Telegram Port

	•	webhook receiver
	•	sendMessage output

⸻

9) Memory System (MVP Knowledge Graph)

9.1 Data model

Tables:
	•	entities(entity_id PK, type, label, props_json, created_at, updated_at)
	•	relations(rel_id PK, from_entity_id, rel_type, to_entity_id, props_json, created_at)
	•	entity_fts (SQLite FTS) over label + selected props

9.2 Operations
	•	search(text, type?, limit?) -> [EntitySummary]
	•	get(entity_id) -> Entity
	•	upsert(type, natural_key?, label, props) -> entity_id
	•	link(from, rel_type, to, props?)
	•	timeline(entity_id) -> events (optional)

⸻

10) Control Plane (MVP)

HTTP API (localhost / configurable bind):
	•	GET /health
	•	GET /tasks?status=&limit=
	•	GET /tasks/:id
	•	GET /tasks/:id/events
	•	GET /memory/search?q=
	•	GET /memory/entities/:id
	•	POST /config/reload (optional)

Auth: none in v0 or simple token.

⸻

11) Security & Safety (Minimum viable)
	•	Sandbox time limit per execute (e.g. 2s CPU)
	•	Memory limit per isolate
	•	execute cannot access host FS/network unless explicitly granted
	•	Tool registry marks capabilities with required permissions
	•	Universe config defines allowed permissions for this deployment

⸻

12) CLI & Deployment

CLI
	•	borg run --universe <path-or-url> --port http:8080
	•	borg migrate (sets up SQLite)
	•	borg doctor (prints config + connectivity)

Container
	•	single image with borg
	•	mount volume for SQLite (Universe)
	•	expose HTTP port(s)

⸻

13) Acceptance Tests (Concrete)
	1.	Send HTTP inbox message “remember I like /media/plex/movies”
	•	agent writes preference entity
	2.	Send “download Minions”
	•	agent searches memory → none
	•	agent calls search() → gets “torrents.search / torrents.download”
	•	agent asks for confirmation or picks default
	•	agent “downloads” (stub) and writes Movie + Torrent entities + relations
	•	output message contains “Downloaded Minions …”
	3.	Send “download Minions” again
	•	agent finds in memory and responds “Already downloaded at …”

⸻

14) Build Order (Fastest path)
	1.	SQLite Universe + Task tables + scheduler loop
	2.	HTTP Port → creates user_message tasks
	3.	Memory tables + FTS search
	4.	Tool registry + search()
	5.	JS sandbox + execute() with memory.* and ports.send
	6.	Very dumb “agent” loop (even rule-based) → later swap to real LLM
	7.	Minimal control plane endpoints
