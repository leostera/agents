import {
  ActorStatusValue,
  GraphQLRequestError,
  requestGraphQL,
  resolveDefaultBaseUrl,
  upsertOnboardingActor,
} from "@borg/graphql-client";
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
  ScrollArea,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Textarea,
} from "@borg/ui";
import React from "react";

const STORAGE_KEY = "borg.devmode.local-state.v1";
const DEFAULT_NAMESPACE = "borg:*";

type ActivityKind =
  | "system"
  | "action"
  | "received"
  | "responded"
  | "tool_call";

type ActivityEntry = {
  id: string;
  at: string;
  kind: ActivityKind;
  summary: string;
  detail?: string;
  actorId?: string;
  actorName?: string;
  taskId?: string;
};

type ScannerStatus = "queued" | "running" | "ready" | "error";

type WorkspaceScanner = {
  actorId: string;
  status: ScannerStatus;
  priorities: string[];
  lastScanAt: string | null;
};

type AgentRole = "Product Manager" | "Designer" | "Engineer" | "Custom";

type WorkspaceAgent = {
  id: string;
  name: string;
  role: AgentRole;
  prompt: string;
  personality: string;
  provider: string;
  model: string;
  tools: string[];
  createdAt: string;
  updatedAt: string;
  activity: ActivityEntry[];
};

type TaskStatus = "backlog" | "in_progress" | "review" | "done";

type SubtaskStatus = "todo" | "doing" | "done";

type TaskSubtask = {
  id: string;
  title: string;
  ownerAgentId: string | null;
  status: SubtaskStatus;
};

type TaskComment = {
  id: string;
  taskId: string;
  createdAt: string;
  authorId: string | null;
  authorName: string;
  authorKind: "user" | "agent" | "system";
  body: string;
};

type TaskRecord = {
  id: string;
  identifier: string;
  title: string;
  description: string;
  labels: string[];
  status: TaskStatus;
  assigneeAgentId: string | null;
  createdAt: string;
  updatedAt: string;
  comments: TaskComment[];
  subtasks: TaskSubtask[];
  activity: ActivityEntry[];
};

type WorkspaceRecord = {
  id: string;
  name: string;
  icon: string;
  namespace: string;
  projectRoot: string;
  createdAt: string;
  updatedAt: string;
  taskSequence: number;
  scanner: WorkspaceScanner;
  agents: WorkspaceAgent[];
  tasks: TaskRecord[];
  activity: ActivityEntry[];
};

type ViewState =
  | { type: "tasks" }
  | { type: "task"; taskId: string }
  | { type: "agent"; agentId: string };

type DevModeStore = {
  version: 1;
  selectedWorkspaceId: string | null;
  view: ViewState;
  workspaces: WorkspaceRecord[];
};

type ApiStatus = {
  state: "checking" | "online" | "offline";
  message: string;
  baseUrl: string;
  checkedAt: string | null;
};

type TaskDraft = {
  title: string;
  description: string;
  labels: string;
};

type WorkspaceDraft = {
  name: string;
  namespace: string;
  projectRoot: string;
};

type AgentEditor = {
  name: string;
  role: AgentRole;
  personality: string;
  provider: string;
  model: string;
  tools: string;
  prompt: string;
};

type AgentDraft = AgentEditor;

const DOC_SCAN_PRIORITIES = [
  "README*",
  "ARCHITECTURE*",
  "HACKING*",
  "docs/**",
  "RFC*",
  "RFD*",
];

const DEFAULT_WORKSPACE_DRAFT: WorkspaceDraft = {
  name: "",
  namespace: DEFAULT_NAMESPACE,
  projectRoot: "",
};

const DEFAULT_TASK_DRAFT: TaskDraft = {
  title: "",
  description: "",
  labels: "",
};

const DEFAULT_AGENT_DRAFT: AgentDraft = {
  name: "",
  role: "Custom",
  personality: "Direct, calm, and concrete.",
  provider: "openai",
  model: "gpt-5",
  tools: "repo.search, repo.edit, terminal.exec",
  prompt: "",
};

const TASK_STATUS_OPTIONS: Array<{ value: TaskStatus; label: string }> = [
  { value: "backlog", label: "Backlog" },
  { value: "in_progress", label: "In Progress" },
  { value: "review", label: "Review" },
  { value: "done", label: "Done" },
];

const SUBTASK_STATUS_OPTIONS: Record<
  SubtaskStatus,
  { label: string; className: string }
> = {
  todo: { label: "Todo", className: "bg-zinc-500/10 text-zinc-600" },
  doing: { label: "Doing", className: "bg-amber-500/10 text-amber-700" },
  done: { label: "Done", className: "bg-emerald-500/10 text-emerald-700" },
};

const TASK_STATUS_BADGE: Record<TaskStatus, string> = {
  backlog: "bg-zinc-500/10 text-zinc-600",
  in_progress: "bg-sky-500/12 text-sky-700",
  review: "bg-amber-500/12 text-amber-700",
  done: "bg-emerald-500/12 text-emerald-700",
};

const ACTIVITY_KIND_BADGE: Record<ActivityKind, string> = {
  system: "bg-zinc-500/10 text-zinc-700",
  action: "bg-sky-500/12 text-sky-700",
  received: "bg-indigo-500/12 text-indigo-700",
  responded: "bg-emerald-500/12 text-emerald-700",
  tool_call: "bg-amber-500/12 text-amber-700",
};

const DEFAULT_STORE: DevModeStore = {
  version: 1,
  selectedWorkspaceId: null,
  view: { type: "tasks" },
  workspaces: [],
};

function createId(prefix: string): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return `${prefix}-${crypto.randomUUID()}`;
  }
  return `${prefix}-${Date.now()}-${Math.floor(Math.random() * 1000)}`;
}

function isoNow(): string {
  return new Date().toISOString();
}

function formatDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
}

function createActivity(
  payload: Omit<ActivityEntry, "id" | "at"> & {
    at?: string;
  }
): ActivityEntry {
  return {
    id: createId("activity"),
    at: payload.at ?? isoNow(),
    kind: payload.kind,
    summary: payload.summary,
    detail: payload.detail,
    actorId: payload.actorId,
    actorName: payload.actorName,
    taskId: payload.taskId,
  };
}

function parseLabels(raw: string): string[] {
  const labels = raw
    .split(",")
    .map((label) => label.trim().replace(/\s+/g, " "))
    .filter((label) => label.length > 0);
  return Array.from(new Set(labels)).slice(0, 8);
}

function parseTools(raw: string): string[] {
  const tools = raw
    .split(",")
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0);
  return Array.from(new Set(tools));
}

function deriveIcon(name: string): string {
  const clean = name.trim();
  if (!clean) return "?";
  return clean.charAt(0).toUpperCase();
}

function normalizeNamespace(raw: string): string {
  const value = raw.trim();
  return value.length > 0 ? value : DEFAULT_NAMESPACE;
}

function buildNamespacePrefix(namespaceValue: string): string {
  const normalized = normalizeNamespace(namespaceValue)
    .toLowerCase()
    .replace("*", "workspace")
    .replace(/[^a-z0-9:_-]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$|:\z/g, "");
  return normalized.length > 0 ? normalized : "borg:workspace";
}

function buildActorId(
  namespaceValue: string,
  workspaceId: string,
  key: string
): string {
  const suffix = workspaceId.split("-").at(-1) ?? workspaceId;
  return `${buildNamespacePrefix(namespaceValue)}:actor:${key}:${suffix}`;
}

function taskStatusLabel(status: TaskStatus): string {
  return (
    TASK_STATUS_OPTIONS.find((option) => option.value === status)?.label ??
    status
  );
}

function cycleSubtaskStatus(status: SubtaskStatus): SubtaskStatus {
  if (status === "todo") return "doing";
  if (status === "doing") return "done";
  return "todo";
}

function loadStore(): DevModeStore {
  if (typeof window === "undefined") {
    return DEFAULT_STORE;
  }
  const raw = window.localStorage.getItem(STORAGE_KEY);
  if (!raw) {
    return DEFAULT_STORE;
  }
  try {
    const parsed = JSON.parse(raw) as Partial<DevModeStore>;
    if (parsed.version !== 1 || !Array.isArray(parsed.workspaces)) {
      return DEFAULT_STORE;
    }
    return {
      version: 1,
      selectedWorkspaceId:
        typeof parsed.selectedWorkspaceId === "string"
          ? parsed.selectedWorkspaceId
          : null,
      view:
        parsed.view && typeof parsed.view === "object" && "type" in parsed.view
          ? (parsed.view as ViewState)
          : { type: "tasks" },
      workspaces: parsed.workspaces,
    };
  } catch {
    return DEFAULT_STORE;
  }
}

function persistStore(store: DevModeStore): void {
  if (typeof window === "undefined") {
    return;
  }
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(store));
}

function updateWorkspaceInStore(
  store: DevModeStore,
  workspaceId: string,
  updater: (workspace: WorkspaceRecord) => WorkspaceRecord
): DevModeStore {
  const index = store.workspaces.findIndex(
    (workspace) => workspace.id === workspaceId
  );
  if (index === -1) {
    return store;
  }
  const nextWorkspaces = [...store.workspaces];
  nextWorkspaces[index] = updater(nextWorkspaces[index]);
  return {
    ...store,
    workspaces: nextWorkspaces,
  };
}

function updateTaskInWorkspace(
  workspace: WorkspaceRecord,
  taskId: string,
  updater: (task: TaskRecord) => TaskRecord
): WorkspaceRecord {
  let touched = false;
  const nextTasks = workspace.tasks.map((task) => {
    if (task.id !== taskId) return task;
    touched = true;
    return updater(task);
  });

  if (!touched) {
    return workspace;
  }

  return {
    ...workspace,
    tasks: nextTasks,
    updatedAt: isoNow(),
  };
}

function createDefaultAgents(
  namespaceValue: string,
  workspaceId: string
): WorkspaceAgent[] {
  const createdAt = isoNow();

  const templates: Array<{
    key: string;
    name: string;
    role: AgentRole;
    personality: string;
    provider: string;
    model: string;
    tools: string[];
    prompt: string;
  }> = [
    {
      key: "product-manager",
      name: "Product Manager",
      role: "Product Manager",
      personality: "Fast, strategic, and ruthless about scope.",
      provider: "openai",
      model: "gpt-5",
      tools: ["taskgraph.plan", "taskgraph.comment", "repo.search"],
      prompt:
        "You are the workspace Product Manager. Clarify desired outcomes, protect scope boundaries, and propose task slices that can ship in parallel.",
    },
    {
      key: "designer",
      name: "Designer",
      role: "Designer",
      personality: "Detail-focused, systems-minded, and UX-first.",
      provider: "openai",
      model: "gpt-5",
      tools: ["ui.review", "ui.prototype", "taskgraph.comment"],
      prompt:
        "You are the workspace Designer. Keep interactions keyboard-first, dense, and low-friction. Suggest interface constraints and usability checks before implementation.",
    },
    {
      key: "engineer",
      name: "Engineer",
      role: "Engineer",
      personality: "Pragmatic, execution-oriented, and explicit about risk.",
      provider: "openai",
      model: "gpt-5",
      tools: ["repo.search", "repo.edit", "terminal.exec", "taskgraph.comment"],
      prompt:
        "You are the workspace Engineer. Translate product/design intent into implementation slices with concrete sequencing, data contracts, and test strategy.",
    },
  ];

  return templates.map((template) => {
    const actorId = buildActorId(namespaceValue, workspaceId, template.key);
    return {
      id: actorId,
      name: template.name,
      role: template.role,
      personality: template.personality,
      provider: template.provider,
      model: template.model,
      tools: template.tools,
      prompt: template.prompt,
      createdAt,
      updatedAt: createdAt,
      activity: [
        createActivity({
          kind: "system",
          actorId,
          actorName: template.name,
          summary: "Agent profile seeded for workspace.",
        }),
      ],
    };
  });
}

function buildRuntimePrompt(
  provider: string,
  model: string,
  prompt: string
): string {
  const normalizedProvider = provider.trim();
  const normalizedModel = model.trim();
  const normalizedPrompt = prompt.trim();

  const lines: string[] = [];
  if (normalizedProvider.length > 0) {
    lines.push(`[runtime.provider] ${normalizedProvider}`);
  }
  if (normalizedModel.length > 0) {
    lines.push(`[runtime.model] ${normalizedModel}`);
  }
  if (lines.length > 0 && normalizedPrompt.length > 0) {
    lines.push("");
  }
  lines.push(normalizedPrompt);
  return lines.join("\n");
}

function getTrioAgents(agents: WorkspaceAgent[]): WorkspaceAgent[] {
  const roleOrder: AgentRole[] = ["Product Manager", "Designer", "Engineer"];
  const picked: WorkspaceAgent[] = [];

  for (const role of roleOrder) {
    const match = agents.find((agent) => agent.role === role);
    if (match) picked.push(match);
  }

  if (picked.length < 3) {
    const fallback = agents.filter(
      (agent) => !picked.some((entry) => entry.id === agent.id)
    );
    picked.push(...fallback.slice(0, 3 - picked.length));
  }

  return picked.slice(0, 3);
}

function buildAgentReview(
  agent: WorkspaceAgent,
  task: Pick<TaskRecord, "id" | "identifier" | "title" | "description">
): {
  comment: string;
  subtasks: string[];
} {
  if (agent.role === "Product Manager") {
    return {
      comment: [
        "Scope review:",
        `- Keep ${task.identifier} focused on one user-visible outcome.`,
        "- Define acceptance criteria before coding.",
        "- Flag external dependencies early so execution can run in parallel.",
      ].join("\n"),
      subtasks: [
        "Write outcome-oriented acceptance criteria.",
        "Identify constraints and non-goals.",
      ],
    };
  }

  if (agent.role === "Designer") {
    return {
      comment: [
        "UX review:",
        "- Start from keyboard-first flows.",
        "- Keep views dense but legible with clear hierarchy.",
        "- Define empty/loading/error behavior before polishing visuals.",
      ].join("\n"),
      subtasks: [
        "Draft interaction map and navigation states.",
        "Define visual hierarchy and activity-log readability checks.",
      ],
    };
  }

  return {
    comment: [
      "Implementation review:",
      "- Start local-first and sync side effects to the runtime.",
      "- Split work into independent slices for parallel execution.",
      "- Add instrumentation for task and agent timeline events.",
    ].join("\n"),
    subtasks: [
      "Implement data model and local persistence.",
      "Wire runtime actor sync and failure handling.",
      "Add activity timeline events for comments/actions/tool calls.",
    ],
  };
}

function buildAutomaticTaskReview(
  task: Pick<TaskRecord, "id" | "identifier" | "title" | "description">,
  trio: WorkspaceAgent[]
): {
  comments: TaskComment[];
  subtasks: TaskSubtask[];
  taskActivity: ActivityEntry[];
  agentEvents: Record<string, ActivityEntry[]>;
} {
  const comments: TaskComment[] = [];
  const taskActivity: ActivityEntry[] = [];
  const agentEvents: Record<string, ActivityEntry[]> = {};
  const subtasks: TaskSubtask[] = [];
  const seenSubtasks = new Set<string>();

  for (const agent of trio) {
    const review = buildAgentReview(agent, task);
    const createdAt = isoNow();

    comments.push({
      id: createId("comment"),
      taskId: task.id,
      createdAt,
      authorId: agent.id,
      authorName: agent.name,
      authorKind: "agent",
      body: review.comment,
    });

    taskActivity.push(
      createActivity({
        kind: "received",
        actorId: agent.id,
        actorName: agent.name,
        summary: `${agent.name} received ${task.identifier} for triage.`,
        taskId: task.id,
      })
    );

    taskActivity.push(
      createActivity({
        kind: "tool_call",
        actorId: agent.id,
        actorName: agent.name,
        summary: `${agent.name} ran task.breakdown.`,
        detail: `Generated ${review.subtasks.length} suggestions.`,
        taskId: task.id,
      })
    );

    taskActivity.push(
      createActivity({
        kind: "responded",
        actorId: agent.id,
        actorName: agent.name,
        summary: `${agent.name} posted review comments.`,
        taskId: task.id,
      })
    );

    agentEvents[agent.id] = [
      createActivity({
        kind: "received",
        actorId: agent.id,
        actorName: agent.name,
        summary: `Received ${task.identifier} for review.`,
        detail: task.title,
        taskId: task.id,
      }),
      createActivity({
        kind: "tool_call",
        actorId: agent.id,
        actorName: agent.name,
        summary: "Executed task.breakdown tool call.",
        taskId: task.id,
      }),
      createActivity({
        kind: "responded",
        actorId: agent.id,
        actorName: agent.name,
        summary: `Published feedback on ${task.identifier}.`,
        taskId: task.id,
      }),
    ];

    for (const suggestion of review.subtasks) {
      const key = suggestion.toLowerCase();
      if (seenSubtasks.has(key)) {
        continue;
      }
      seenSubtasks.add(key);
      subtasks.push({
        id: createId("subtask"),
        title: suggestion,
        ownerAgentId: agent.id,
        status: "todo",
      });
    }
  }

  return {
    comments,
    subtasks,
    taskActivity,
    agentEvents,
  };
}

function buildTaskTimeline(task: TaskRecord): ActivityEntry[] {
  const commentEntries = task.comments.map((comment) =>
    createActivity({
      at: comment.createdAt,
      kind: comment.authorKind === "agent" ? "responded" : "action",
      actorId: comment.authorId ?? undefined,
      actorName: comment.authorName,
      summary: `${comment.authorName} commented.`,
      detail: comment.body,
      taskId: task.id,
    })
  );

  return [...task.activity, ...commentEntries].sort((left, right) =>
    right.at.localeCompare(left.at)
  );
}

function copyWorkspaceDraft(draft: WorkspaceDraft): WorkspaceDraft {
  return {
    name: draft.name,
    namespace: draft.namespace,
    projectRoot: draft.projectRoot,
  };
}

function normalizeRole(raw: string): AgentRole {
  if (raw === "Product Manager" || raw === "Designer" || raw === "Engineer") {
    return raw;
  }
  return "Custom";
}

function statusMessage(status: ApiStatus): string {
  if (status.state === "online") {
    return `GraphQL runtime reachable at ${status.baseUrl}`;
  }
  if (status.state === "checking") {
    return "Checking Borg GraphQL runtime...";
  }
  return status.message || "GraphQL runtime is unreachable.";
}

export function App() {
  const [store, setStore] = React.useState<DevModeStore>(() => loadStore());
  const [apiStatus, setApiStatus] = React.useState<ApiStatus>({
    state: "checking",
    message: "Checking Borg GraphQL runtime...",
    baseUrl: resolveDefaultBaseUrl(),
    checkedAt: null,
  });
  const [notice, setNotice] = React.useState<string | null>(null);

  const [isWorkspaceDialogOpen, setIsWorkspaceDialogOpen] =
    React.useState(false);
  const [workspaceDraft, setWorkspaceDraft] = React.useState<WorkspaceDraft>(
    copyWorkspaceDraft(DEFAULT_WORKSPACE_DRAFT)
  );

  const [isTaskDialogOpen, setIsTaskDialogOpen] = React.useState(false);
  const [taskDraft, setTaskDraft] = React.useState<TaskDraft>({
    ...DEFAULT_TASK_DRAFT,
  });

  const [isAgentDialogOpen, setIsAgentDialogOpen] = React.useState(false);
  const [agentDraft, setAgentDraft] = React.useState<AgentDraft>({
    ...DEFAULT_AGENT_DRAFT,
  });

  const [taskCommentDraft, setTaskCommentDraft] = React.useState("");
  const [taskLabelDraft, setTaskLabelDraft] = React.useState("");
  const [agentEditor, setAgentEditor] = React.useState<AgentEditor | null>(
    null
  );

  const bootstrapInFlightRef = React.useRef(new Set<string>());

  const activeWorkspace = React.useMemo(
    () =>
      store.workspaces.find(
        (workspace) => workspace.id === store.selectedWorkspaceId
      ) ?? null,
    [store.workspaces, store.selectedWorkspaceId]
  );

  const selectedTask = React.useMemo(() => {
    if (!activeWorkspace) return null;
    const currentView = store.view;
    if (currentView.type !== "task") return null;
    return (
      activeWorkspace.tasks.find((task) => task.id === currentView.taskId) ??
      null
    );
  }, [activeWorkspace, store.view]);

  const selectedAgent = React.useMemo(() => {
    if (!activeWorkspace) return null;
    const currentView = store.view;
    if (currentView.type !== "agent") return null;
    return (
      activeWorkspace.agents.find(
        (agent) => agent.id === currentView.agentId
      ) ?? null
    );
  }, [activeWorkspace, store.view]);

  React.useEffect(() => {
    persistStore(store);
  }, [store]);

  React.useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(null), 5000);
    return () => clearTimeout(timer);
  }, [notice]);

  React.useEffect(() => {
    if (!store.selectedWorkspaceId && store.workspaces.length > 0) {
      setStore((previous) => ({
        ...previous,
        selectedWorkspaceId: previous.workspaces[0]?.id ?? null,
        view: { type: "tasks" },
      }));
      return;
    }

    if (
      store.selectedWorkspaceId &&
      !store.workspaces.some(
        (workspace) => workspace.id === store.selectedWorkspaceId
      )
    ) {
      setStore((previous) => ({
        ...previous,
        selectedWorkspaceId: previous.workspaces[0]?.id ?? null,
        view: { type: "tasks" },
      }));
    }
  }, [store.selectedWorkspaceId, store.workspaces]);

  React.useEffect(() => {
    if (!activeWorkspace) return;
    const currentView = store.view;
    if (currentView.type === "task") {
      const exists = activeWorkspace.tasks.some(
        (task) => task.id === currentView.taskId
      );
      if (!exists) {
        setStore((previous) => ({ ...previous, view: { type: "tasks" } }));
      }
    }
    if (currentView.type === "agent") {
      const exists = activeWorkspace.agents.some(
        (agent) => agent.id === currentView.agentId
      );
      if (!exists) {
        setStore((previous) => ({ ...previous, view: { type: "tasks" } }));
      }
    }
  }, [activeWorkspace, store.view]);

  React.useEffect(() => {
    if (!selectedAgent) {
      setAgentEditor(null);
      return;
    }
    setAgentEditor({
      name: selectedAgent.name,
      role: selectedAgent.role,
      personality: selectedAgent.personality,
      provider: selectedAgent.provider,
      model: selectedAgent.model,
      tools: selectedAgent.tools.join(", "),
      prompt: selectedAgent.prompt,
    });
  }, [selectedAgent?.id]);

  const checkApiHealth = React.useCallback(async () => {
    const baseUrl = resolveDefaultBaseUrl();
    setApiStatus((previous) => ({
      ...previous,
      state: "checking",
      baseUrl,
      message: "Checking Borg GraphQL runtime...",
    }));

    try {
      await requestGraphQL<{ __typename: string }>({
        query: "query DevModeHealth { __typename }",
      });
      setApiStatus({
        state: "online",
        baseUrl,
        message: "GraphQL runtime reachable.",
        checkedAt: isoNow(),
      });
    } catch (error) {
      const message =
        error instanceof GraphQLRequestError
          ? error.message
          : error instanceof Error
            ? error.message
            : "Unable to reach GraphQL runtime";
      setApiStatus({
        state: "offline",
        baseUrl,
        message,
        checkedAt: isoNow(),
      });
    }
  }, []);

  React.useEffect(() => {
    void checkApiHealth();
    const timer = setInterval(() => {
      void checkApiHealth();
    }, 30000);
    return () => clearInterval(timer);
  }, [checkApiHealth]);

  const bootstrapWorkspaceRuntime = React.useCallback(
    async (workspace: WorkspaceRecord) => {
      if (workspace.scanner.status !== "queued") return;
      if (bootstrapInFlightRef.current.has(workspace.id)) return;

      bootstrapInFlightRef.current.add(workspace.id);

      setStore((previous) =>
        updateWorkspaceInStore(previous, workspace.id, (current) => ({
          ...current,
          scanner: {
            ...current.scanner,
            status: "running",
          },
          activity: [
            createActivity({
              kind: "action",
              summary: "Runtime actor synchronization started.",
              detail: `Registering ${current.agents.length} team actors through GraphQL. Scanner bootstrap is disabled.`,
            }),
            ...current.activity,
          ],
        }))
      );

      try {
        for (const agent of workspace.agents) {
          await upsertOnboardingActor({
            actorId: agent.id,
            name: agent.name,
            systemPrompt: buildRuntimePrompt(
              agent.provider,
              agent.model,
              agent.prompt
            ),
            status: ActorStatusValue.Running,
          });
        }

        setStore((previous) =>
          updateWorkspaceInStore(previous, workspace.id, (current) => ({
            ...current,
            scanner: {
              ...current.scanner,
              status: "ready",
              lastScanAt: isoNow(),
            },
            activity: [
              createActivity({
                kind: "system",
                summary: "Runtime actors synchronized.",
                detail: `${current.agents.length} team agents are now registered for workspace ${current.namespace}.`,
              }),
              ...current.activity,
            ],
          }))
        );
      } catch (error) {
        const message =
          error instanceof Error
            ? error.message
            : "Unable to bootstrap workspace actors";
        setStore((previous) =>
          updateWorkspaceInStore(previous, workspace.id, (current) => ({
            ...current,
            scanner: {
              ...current.scanner,
              status: "error",
            },
            activity: [
              createActivity({
                kind: "system",
                summary: "Runtime bootstrap failed.",
                detail: message,
              }),
              ...current.activity,
            ],
          }))
        );
        setNotice(`Failed to sync workspace actors: ${message}`);
      } finally {
        bootstrapInFlightRef.current.delete(workspace.id);
      }
    },
    []
  );

  React.useEffect(() => {
    for (const workspace of store.workspaces) {
      if (workspace.scanner.status === "queued") {
        void bootstrapWorkspaceRuntime(workspace);
      }
    }
  }, [bootstrapWorkspaceRuntime, store.workspaces]);

  const syncAgentRuntime = React.useCallback(
    async (agent: WorkspaceAgent) => {
      try {
        await upsertOnboardingActor({
          actorId: agent.id,
          name: agent.name,
          systemPrompt: buildRuntimePrompt(
            agent.provider,
            agent.model,
            agent.prompt
          ),
          status: ActorStatusValue.Running,
        });
      } catch (error) {
        const message =
          error instanceof Error
            ? error.message
            : "Unable to sync agent runtime profile";
        setNotice(`Failed to sync agent ${agent.name}: ${message}`);
      }
    },
    [setNotice]
  );

  const selectWorkspace = React.useCallback((workspaceId: string) => {
    setStore((previous) => ({
      ...previous,
      selectedWorkspaceId: workspaceId,
      view: { type: "tasks" },
    }));
  }, []);

  const handleCreateWorkspace = React.useCallback(() => {
    const name = workspaceDraft.name.trim();
    const namespaceValue = normalizeNamespace(workspaceDraft.namespace);
    const projectRoot = workspaceDraft.projectRoot.trim();

    if (!name) {
      setNotice("Workspace name is required.");
      return;
    }
    if (!projectRoot) {
      setNotice(
        "Project root is required so scanner actors know what to index."
      );
      return;
    }

    const workspaceId = createId("workspace");
    const createdAt = isoNow();
    const agents = createDefaultAgents(namespaceValue, workspaceId);
    const scannerActorId = buildActorId(namespaceValue, workspaceId, "scanner");

    const workspace: WorkspaceRecord = {
      id: workspaceId,
      name,
      icon: deriveIcon(name),
      namespace: namespaceValue,
      projectRoot,
      createdAt,
      updatedAt: createdAt,
      taskSequence: 1,
      scanner: {
        actorId: scannerActorId,
        status: "queued",
        priorities: [...DOC_SCAN_PRIORITIES],
        lastScanAt: null,
      },
      agents,
      tasks: [],
      activity: [
        createActivity({
          kind: "action",
          summary: "Workspace created.",
          detail: `${name} initialized with namespace ${namespaceValue}.`,
        }),
        createActivity({
          kind: "action",
          summary: "Scanner queued.",
          detail: `Scanner actor ${scannerActorId} will prioritize ${DOC_SCAN_PRIORITIES.join(", ")}.`,
        }),
        createActivity({
          kind: "system",
          summary: "Team preconfigured.",
          detail:
            "Product Manager, Designer, and Engineer profiles were seeded.",
        }),
      ],
    };

    setStore((previous) => ({
      ...previous,
      workspaces: [workspace, ...previous.workspaces],
      selectedWorkspaceId: workspaceId,
      view: { type: "tasks" },
    }));

    setWorkspaceDraft(copyWorkspaceDraft(DEFAULT_WORKSPACE_DRAFT));
    setIsWorkspaceDialogOpen(false);
  }, [workspaceDraft]);

  const handleCreateTask = React.useCallback(() => {
    const title = taskDraft.title.trim();
    const description = taskDraft.description.trim();
    const labels = parseLabels(taskDraft.labels);

    if (!title) {
      setNotice("Task title is required.");
      return;
    }

    setStore((previous) => {
      const workspaceId = previous.selectedWorkspaceId;
      if (!workspaceId) return previous;

      let createdTaskId: string | null = null;

      const nextStore = updateWorkspaceInStore(
        previous,
        workspaceId,
        (workspace) => {
          const createdAt = isoNow();
          const taskId = createId("task");
          const identifier = `T-${workspace.taskSequence}`;

          const taskSeed: TaskRecord = {
            id: taskId,
            identifier,
            title,
            description,
            labels,
            status: "backlog",
            assigneeAgentId: null,
            createdAt,
            updatedAt: createdAt,
            comments: [],
            subtasks: [],
            activity: [
              createActivity({
                kind: "action",
                summary: "Task created.",
                detail: title,
                taskId,
              }),
            ],
          };

          const trio = getTrioAgents(workspace.agents);
          const review = buildAutomaticTaskReview(taskSeed, trio);

          const finalizedTask: TaskRecord = {
            ...taskSeed,
            comments: review.comments,
            subtasks: review.subtasks,
            activity: [...review.taskActivity, ...taskSeed.activity],
            updatedAt: isoNow(),
          };

          createdTaskId = taskId;

          const updatedAgents = workspace.agents.map((agent) => {
            const events = review.agentEvents[agent.id];
            if (!events) return agent;
            return {
              ...agent,
              updatedAt: isoNow(),
              activity: [...events, ...agent.activity],
            };
          });

          return {
            ...workspace,
            taskSequence: workspace.taskSequence + 1,
            updatedAt: isoNow(),
            tasks: [finalizedTask, ...workspace.tasks],
            agents: updatedAgents,
            activity: [
              createActivity({
                kind: "action",
                summary: `Task ${identifier} created.`,
                detail:
                  "Trio review comments and suggested subtasks were generated automatically.",
                taskId,
              }),
              ...workspace.activity,
            ],
          };
        }
      );

      if (!createdTaskId) {
        return nextStore;
      }

      return {
        ...nextStore,
        view: {
          type: "task",
          taskId: createdTaskId,
        },
      };
    });

    setTaskDraft({ ...DEFAULT_TASK_DRAFT });
    setIsTaskDialogOpen(false);
  }, [taskDraft]);

  const handleTaskStatusChange = React.useCallback(
    (taskId: string, status: TaskStatus) => {
      if (!store.selectedWorkspaceId) return;

      setStore((previous) =>
        updateWorkspaceInStore(
          previous,
          store.selectedWorkspaceId as string,
          (workspace) => {
            let changedIdentifier = "";
            const nextWorkspace = updateTaskInWorkspace(
              workspace,
              taskId,
              (task) => {
                if (task.status === status) return task;
                changedIdentifier = task.identifier;
                return {
                  ...task,
                  status,
                  updatedAt: isoNow(),
                  activity: [
                    createActivity({
                      kind: "action",
                      summary: `Status changed to ${taskStatusLabel(status)}.`,
                      taskId: task.id,
                    }),
                    ...task.activity,
                  ],
                };
              }
            );

            if (!changedIdentifier) {
              return workspace;
            }

            return {
              ...nextWorkspace,
              updatedAt: isoNow(),
              activity: [
                createActivity({
                  kind: "action",
                  summary: `${changedIdentifier} moved to ${taskStatusLabel(status)}.`,
                  taskId,
                }),
                ...nextWorkspace.activity,
              ],
            };
          }
        )
      );
    },
    [store.selectedWorkspaceId]
  );

  const handleTaskAssigneeChange = React.useCallback(
    (taskId: string, assigneeAgentId: string | null) => {
      if (!store.selectedWorkspaceId) return;

      setStore((previous) =>
        updateWorkspaceInStore(
          previous,
          store.selectedWorkspaceId as string,
          (workspace) => {
            const agentName =
              assigneeAgentId === null
                ? "Unassigned"
                : (workspace.agents.find(
                    (agent) => agent.id === assigneeAgentId
                  )?.name ?? "Unknown");

            let changedIdentifier = "";
            const nextWorkspace = updateTaskInWorkspace(
              workspace,
              taskId,
              (task) => {
                if (task.assigneeAgentId === assigneeAgentId) return task;
                changedIdentifier = task.identifier;
                return {
                  ...task,
                  assigneeAgentId,
                  updatedAt: isoNow(),
                  activity: [
                    createActivity({
                      kind: "action",
                      summary: `Assignee changed to ${agentName}.`,
                      taskId: task.id,
                    }),
                    ...task.activity,
                  ],
                };
              }
            );

            const nextAgents = nextWorkspace.agents.map((agent) => {
              if (agent.id !== assigneeAgentId) return agent;
              return {
                ...agent,
                updatedAt: isoNow(),
                activity: [
                  createActivity({
                    kind: "received",
                    actorId: agent.id,
                    actorName: agent.name,
                    summary: `Assigned to ${changedIdentifier || "task"}.`,
                    taskId,
                  }),
                  ...agent.activity,
                ],
              };
            });

            if (!changedIdentifier) {
              return workspace;
            }

            return {
              ...nextWorkspace,
              agents: nextAgents,
              updatedAt: isoNow(),
              activity: [
                createActivity({
                  kind: "action",
                  summary: `${changedIdentifier} assigned to ${agentName}.`,
                  taskId,
                }),
                ...nextWorkspace.activity,
              ],
            };
          }
        )
      );
    },
    [store.selectedWorkspaceId]
  );

  const handleTaskCommentCreate = React.useCallback(() => {
    const text = taskCommentDraft.trim();
    if (!text || !selectedTask || !store.selectedWorkspaceId) return;

    setStore((previous) =>
      updateWorkspaceInStore(
        previous,
        store.selectedWorkspaceId as string,
        (workspace) => {
          const now = isoNow();
          const comment: TaskComment = {
            id: createId("comment"),
            taskId: selectedTask.id,
            createdAt: now,
            authorId: null,
            authorName: "You",
            authorKind: "user",
            body: text,
          };

          const nextWorkspace = updateTaskInWorkspace(
            workspace,
            selectedTask.id,
            (task) => ({
              ...task,
              comments: [comment, ...task.comments],
              updatedAt: now,
              activity: [
                createActivity({
                  at: now,
                  kind: "action",
                  summary: "User comment added.",
                  taskId: task.id,
                }),
                ...task.activity,
              ],
            })
          );

          return {
            ...nextWorkspace,
            updatedAt: now,
            activity: [
              createActivity({
                at: now,
                kind: "action",
                summary: `${selectedTask.identifier} received a new comment.`,
                taskId: selectedTask.id,
              }),
              ...nextWorkspace.activity,
            ],
          };
        }
      )
    );

    setTaskCommentDraft("");
  }, [selectedTask, store.selectedWorkspaceId, taskCommentDraft]);

  const handleTaskAddLabel = React.useCallback(() => {
    const label = taskLabelDraft.trim();
    if (!label || !selectedTask || !store.selectedWorkspaceId) return;

    setStore((previous) =>
      updateWorkspaceInStore(
        previous,
        store.selectedWorkspaceId as string,
        (workspace) => {
          const now = isoNow();
          return updateTaskInWorkspace(workspace, selectedTask.id, (task) => {
            if (task.labels.includes(label)) return task;
            return {
              ...task,
              labels: [...task.labels, label],
              updatedAt: now,
              activity: [
                createActivity({
                  kind: "action",
                  summary: `Label added: ${label}.`,
                  taskId: task.id,
                }),
                ...task.activity,
              ],
            };
          });
        }
      )
    );

    setTaskLabelDraft("");
  }, [selectedTask, store.selectedWorkspaceId, taskLabelDraft]);

  const handleTaskToggleSubtask = React.useCallback(
    (taskId: string, subtaskId: string) => {
      if (!store.selectedWorkspaceId) return;

      setStore((previous) =>
        updateWorkspaceInStore(
          previous,
          store.selectedWorkspaceId as string,
          (workspace) =>
            updateTaskInWorkspace(workspace, taskId, (task) => ({
              ...task,
              updatedAt: isoNow(),
              subtasks: task.subtasks.map((subtask) => {
                if (subtask.id !== subtaskId) return subtask;
                return {
                  ...subtask,
                  status: cycleSubtaskStatus(subtask.status),
                };
              }),
              activity: [
                createActivity({
                  kind: "action",
                  summary: "Subtask status updated.",
                  taskId: task.id,
                }),
                ...task.activity,
              ],
            }))
        )
      );
    },
    [store.selectedWorkspaceId]
  );

  const handleCreateAgent = React.useCallback(() => {
    if (!store.selectedWorkspaceId) return;

    const name = agentDraft.name.trim();
    const prompt = agentDraft.prompt.trim();
    if (!name) {
      setNotice("Agent name is required.");
      return;
    }
    if (!prompt) {
      setNotice("Agent prompt is required.");
      return;
    }

    const provider = agentDraft.provider.trim() || "openai";
    const model = agentDraft.model.trim() || "gpt-5";
    const personality =
      agentDraft.personality.trim() || "Direct, factual, concise.";
    const tools = parseTools(agentDraft.tools);

    let createdAgent: WorkspaceAgent | null = null;

    setStore((previous) =>
      updateWorkspaceInStore(
        previous,
        store.selectedWorkspaceId as string,
        (workspace) => {
          const now = isoNow();
          const id = buildActorId(
            workspace.namespace,
            workspace.id,
            `agent-${createId("custom")}`
          );

          const agent: WorkspaceAgent = {
            id,
            name,
            role: normalizeRole(agentDraft.role),
            personality,
            provider,
            model,
            tools,
            prompt,
            createdAt: now,
            updatedAt: now,
            activity: [
              createActivity({
                kind: "system",
                actorId: id,
                actorName: name,
                summary: "Agent created from DevMode team settings.",
              }),
            ],
          };

          createdAgent = agent;

          return {
            ...workspace,
            updatedAt: now,
            agents: [...workspace.agents, agent],
            activity: [
              createActivity({
                kind: "action",
                summary: `Agent added: ${name}.`,
              }),
              ...workspace.activity,
            ],
          };
        }
      )
    );

    if (createdAgent) {
      void syncAgentRuntime(createdAgent);
    }

    setAgentDraft({ ...DEFAULT_AGENT_DRAFT });
    setIsAgentDialogOpen(false);
  }, [agentDraft, store.selectedWorkspaceId, syncAgentRuntime]);

  const handleSaveAgentEditor = React.useCallback(() => {
    if (!activeWorkspace || !selectedAgent || !agentEditor) return;

    const name = agentEditor.name.trim();
    const prompt = agentEditor.prompt.trim();
    if (!name) {
      setNotice("Agent name is required.");
      return;
    }
    if (!prompt) {
      setNotice("Agent prompt is required.");
      return;
    }

    const updatedAgent: WorkspaceAgent = {
      ...selectedAgent,
      name,
      role: normalizeRole(agentEditor.role),
      personality: agentEditor.personality.trim() || selectedAgent.personality,
      provider: agentEditor.provider.trim() || selectedAgent.provider,
      model: agentEditor.model.trim() || selectedAgent.model,
      tools: parseTools(agentEditor.tools),
      prompt,
      updatedAt: isoNow(),
      activity: [
        createActivity({
          kind: "action",
          actorId: selectedAgent.id,
          actorName: name,
          summary: "Agent settings updated.",
        }),
        ...selectedAgent.activity,
      ],
    };

    setStore((previous) =>
      updateWorkspaceInStore(previous, activeWorkspace.id, (workspace) => ({
        ...workspace,
        updatedAt: isoNow(),
        agents: workspace.agents.map((agent) =>
          agent.id === selectedAgent.id ? updatedAgent : agent
        ),
        activity: [
          createActivity({
            kind: "action",
            summary: `Agent settings saved: ${updatedAgent.name}.`,
          }),
          ...workspace.activity,
        ],
      }))
    );

    void syncAgentRuntime(updatedAgent);
  }, [activeWorkspace, agentEditor, selectedAgent, syncAgentRuntime]);

  const renderTasksView = (workspace: WorkspaceRecord) => {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <header className="border-border/70 flex items-center justify-between border-b px-5 py-4">
          <div>
            <p className="text-[11px] uppercase tracking-[0.14em] text-zinc-500">
              Tasks
            </p>
            <h1 className="text-lg font-semibold">{workspace.name}</h1>
          </div>
          <div className="flex items-center gap-2">
            <Badge className="bg-sky-500/12 text-sky-700">Local-first</Badge>
            <Button type="button" onClick={() => setIsTaskDialogOpen(true)}>
              New task
            </Button>
          </div>
        </header>

        <div className="border-border/70 bg-background/85 border-b px-5 py-3 text-xs text-zinc-600">
          <div className="flex flex-wrap items-center gap-2">
            <Badge
              className={
                apiStatus.state === "online"
                  ? "bg-emerald-500/12 text-emerald-700"
                  : apiStatus.state === "checking"
                    ? "bg-amber-500/12 text-amber-700"
                    : "bg-rose-500/12 text-rose-700"
              }
            >
              API {apiStatus.state}
            </Badge>
            <span>{statusMessage(apiStatus)}</span>
            {apiStatus.checkedAt ? (
              <span>Last check: {formatDate(apiStatus.checkedAt)}</span>
            ) : null}
          </div>
        </div>

        <ScrollArea className="min-h-0 flex-1">
          <div className="p-5">
            {workspace.tasks.length === 0 ? (
              <div className="rounded-2xl border border-dashed p-8 text-center">
                <p className="text-sm font-medium">No tasks yet</p>
                <p className="mt-2 text-xs text-zinc-500">
                  Create a task and DevMode will immediately collect
                  PM/Designer/Engineer feedback as comments.
                </p>
                <Button
                  type="button"
                  className="mt-4"
                  variant="outline"
                  onClick={() => setIsTaskDialogOpen(true)}
                >
                  Create first task
                </Button>
              </div>
            ) : (
              <div className="rounded-2xl border bg-white/80">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>ID</TableHead>
                      <TableHead>Title</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Labels</TableHead>
                      <TableHead>Assignee</TableHead>
                      <TableHead>Comments</TableHead>
                      <TableHead>Updated</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {workspace.tasks.map((task) => {
                      const assignee =
                        task.assigneeAgentId === null
                          ? null
                          : (workspace.agents.find(
                              (agent) => agent.id === task.assigneeAgentId
                            ) ?? null);
                      return (
                        <TableRow
                          key={task.id}
                          className="cursor-pointer"
                          onClick={() =>
                            setStore((previous) => ({
                              ...previous,
                              view: { type: "task", taskId: task.id },
                            }))
                          }
                        >
                          <TableCell className="font-medium">
                            {task.identifier}
                          </TableCell>
                          <TableCell>
                            <p className="max-w-[30ch] truncate">
                              {task.title}
                            </p>
                          </TableCell>
                          <TableCell>
                            <Badge className={TASK_STATUS_BADGE[task.status]}>
                              {taskStatusLabel(task.status)}
                            </Badge>
                          </TableCell>
                          <TableCell>
                            <div className="flex max-w-[18rem] flex-wrap gap-1">
                              {task.labels.length === 0 ? (
                                <span className="text-zinc-500">-</span>
                              ) : (
                                task.labels.map((label) => (
                                  <Badge key={label} variant="outline">
                                    {label}
                                  </Badge>
                                ))
                              )}
                            </div>
                          </TableCell>
                          <TableCell>
                            {assignee ? assignee.name : "Unassigned"}
                          </TableCell>
                          <TableCell>{task.comments.length}</TableCell>
                          <TableCell>{formatDate(task.updatedAt)}</TableCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              </div>
            )}
          </div>
        </ScrollArea>
      </div>
    );
  };

  const renderTaskDetail = (workspace: WorkspaceRecord, task: TaskRecord) => {
    const timeline = buildTaskTimeline(task);

    return (
      <div className="flex h-full min-h-0 flex-col">
        <header className="border-border/70 flex items-center justify-between border-b px-5 py-4">
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={() =>
                setStore((previous) => ({
                  ...previous,
                  view: { type: "tasks" },
                }))
              }
            >
              Back
            </Button>
            <div>
              <p className="text-[11px] uppercase tracking-[0.14em] text-zinc-500">
                {task.identifier}
              </p>
              <h1 className="text-lg font-semibold">{task.title}</h1>
            </div>
          </div>
          <Badge className={TASK_STATUS_BADGE[task.status]}>
            {taskStatusLabel(task.status)}
          </Badge>
        </header>

        <ScrollArea className="min-h-0 flex-1">
          <div className="grid gap-4 p-5 lg:grid-cols-[1.3fr_0.7fr]">
            <section className="rounded-2xl border bg-white/85 p-4">
              <p className="text-xs font-semibold uppercase tracking-[0.12em] text-zinc-500">
                Description
              </p>
              <p className="mt-2 whitespace-pre-wrap text-sm text-zinc-700">
                {task.description || "No description yet."}
              </p>

              <div className="mt-5 grid gap-3 md:grid-cols-2">
                <div className="space-y-1.5">
                  <Label>Status</Label>
                  <Select
                    value={task.status}
                    onValueChange={(value) =>
                      handleTaskStatusChange(task.id, value as TaskStatus)
                    }
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select status" />
                    </SelectTrigger>
                    <SelectContent>
                      {TASK_STATUS_OPTIONS.map((option) => (
                        <SelectItem key={option.value} value={option.value}>
                          {option.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>

                <div className="space-y-1.5">
                  <Label>Assignee</Label>
                  <Select
                    value={task.assigneeAgentId ?? "unassigned"}
                    onValueChange={(value) =>
                      handleTaskAssigneeChange(
                        task.id,
                        value === "unassigned" ? null : value
                      )
                    }
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Assign agent" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="unassigned">Unassigned</SelectItem>
                      {workspace.agents.map((agent) => (
                        <SelectItem key={agent.id} value={agent.id}>
                          {agent.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </div>

              <div className="mt-5 space-y-2">
                <Label>Labels</Label>
                <div className="flex flex-wrap gap-1">
                  {task.labels.length === 0 ? (
                    <span className="text-xs text-zinc-500">No labels</span>
                  ) : (
                    task.labels.map((label) => (
                      <Badge key={label} variant="outline">
                        {label}
                      </Badge>
                    ))
                  )}
                </div>
                <div className="flex gap-2">
                  <Input
                    value={taskLabelDraft}
                    onChange={(event) =>
                      setTaskLabelDraft(event.currentTarget.value)
                    }
                    placeholder="Add label"
                  />
                  <Button
                    type="button"
                    variant="outline"
                    onClick={handleTaskAddLabel}
                  >
                    Add
                  </Button>
                </div>
              </div>
            </section>

            <section className="rounded-2xl border bg-white/85 p-4">
              <p className="text-xs font-semibold uppercase tracking-[0.12em] text-zinc-500">
                Subtasks
              </p>
              <div className="mt-2 space-y-2">
                {task.subtasks.length === 0 ? (
                  <p className="text-xs text-zinc-500">No subtasks yet.</p>
                ) : (
                  task.subtasks.map((subtask) => {
                    const owner =
                      subtask.ownerAgentId === null
                        ? null
                        : (workspace.agents.find(
                            (agent) => agent.id === subtask.ownerAgentId
                          ) ?? null);
                    return (
                      <button
                        key={subtask.id}
                        type="button"
                        onClick={() =>
                          handleTaskToggleSubtask(task.id, subtask.id)
                        }
                        className="flex w-full items-center justify-between rounded-xl border px-3 py-2 text-left hover:bg-zinc-50"
                      >
                        <div>
                          <p className="text-sm">{subtask.title}</p>
                          <p className="text-xs text-zinc-500">
                            {owner ? owner.name : "No owner"}
                          </p>
                        </div>
                        <Badge
                          className={
                            SUBTASK_STATUS_OPTIONS[subtask.status].className
                          }
                        >
                          {SUBTASK_STATUS_OPTIONS[subtask.status].label}
                        </Badge>
                      </button>
                    );
                  })
                )}
              </div>
            </section>

            <section className="rounded-2xl border bg-white/85 p-4 lg:col-span-2">
              <p className="text-xs font-semibold uppercase tracking-[0.12em] text-zinc-500">
                Comments
              </p>
              <div className="mt-2 space-y-3">
                {task.comments.length === 0 ? (
                  <p className="text-xs text-zinc-500">No comments yet.</p>
                ) : (
                  task.comments.map((comment) => (
                    <article
                      key={comment.id}
                      className="rounded-xl border px-3 py-2"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <p className="text-xs font-medium">
                          {comment.authorName}
                        </p>
                        <p className="text-[11px] text-zinc-500">
                          {formatDate(comment.createdAt)}
                        </p>
                      </div>
                      <p className="mt-1 whitespace-pre-wrap text-sm text-zinc-700">
                        {comment.body}
                      </p>
                    </article>
                  ))
                )}
              </div>
              <div className="mt-3 space-y-2">
                <Textarea
                  value={taskCommentDraft}
                  onChange={(event) =>
                    setTaskCommentDraft(event.currentTarget.value)
                  }
                  placeholder="Leave a comment"
                  rows={4}
                />
                <div className="flex justify-end">
                  <Button type="button" onClick={handleTaskCommentCreate}>
                    Post comment
                  </Button>
                </div>
              </div>
            </section>

            <section className="rounded-2xl border bg-white/85 p-4 lg:col-span-2">
              <p className="text-xs font-semibold uppercase tracking-[0.12em] text-zinc-500">
                Activity log
              </p>
              <div className="mt-2 space-y-2">
                {timeline.length === 0 ? (
                  <p className="text-xs text-zinc-500">No activity yet.</p>
                ) : (
                  timeline.map((event) => (
                    <article
                      key={event.id}
                      className="rounded-xl border px-3 py-2"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <div className="flex items-center gap-2">
                          <Badge className={ACTIVITY_KIND_BADGE[event.kind]}>
                            {event.kind}
                          </Badge>
                          <p className="text-xs font-medium">{event.summary}</p>
                        </div>
                        <p className="text-[11px] text-zinc-500">
                          {formatDate(event.at)}
                        </p>
                      </div>
                      {event.detail ? (
                        <p className="mt-1 whitespace-pre-wrap text-xs text-zinc-600">
                          {event.detail}
                        </p>
                      ) : null}
                    </article>
                  ))
                )}
              </div>
            </section>
          </div>
        </ScrollArea>
      </div>
    );
  };

  const renderAgentDetail = (
    workspace: WorkspaceRecord,
    agent: WorkspaceAgent
  ) => {
    const editor = agentEditor;

    return (
      <div className="flex h-full min-h-0 flex-col">
        <header className="border-border/70 flex items-center justify-between border-b px-5 py-4">
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={() =>
                setStore((previous) => ({
                  ...previous,
                  view: { type: "tasks" },
                }))
              }
            >
              Back
            </Button>
            <div>
              <p className="text-[11px] uppercase tracking-[0.14em] text-zinc-500">
                Agent
              </p>
              <h1 className="text-lg font-semibold">{agent.name}</h1>
            </div>
          </div>
          <Badge variant="outline">{agent.role}</Badge>
        </header>

        <ScrollArea className="min-h-0 flex-1">
          <div className="grid gap-4 p-5 lg:grid-cols-[0.9fr_1.1fr]">
            <section className="rounded-2xl border bg-white/85 p-4">
              <p className="text-xs font-semibold uppercase tracking-[0.12em] text-zinc-500">
                Config
              </p>
              {editor ? (
                <div className="mt-3 space-y-3">
                  <div className="space-y-1.5">
                    <Label>Name</Label>
                    <Input
                      value={editor.name}
                      onChange={(event) =>
                        setAgentEditor((current) =>
                          current
                            ? {
                                ...current,
                                name: event.currentTarget.value,
                              }
                            : current
                        )
                      }
                    />
                  </div>

                  <div className="grid gap-3 md:grid-cols-2">
                    <div className="space-y-1.5">
                      <Label>Role</Label>
                      <Select
                        value={editor.role}
                        onValueChange={(value) =>
                          setAgentEditor((current) =>
                            current
                              ? {
                                  ...current,
                                  role: normalizeRole(value),
                                }
                              : current
                          )
                        }
                      >
                        <SelectTrigger>
                          <SelectValue placeholder="Role" />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="Product Manager">
                            Product Manager
                          </SelectItem>
                          <SelectItem value="Designer">Designer</SelectItem>
                          <SelectItem value="Engineer">Engineer</SelectItem>
                          <SelectItem value="Custom">Custom</SelectItem>
                        </SelectContent>
                      </Select>
                    </div>

                    <div className="space-y-1.5">
                      <Label>Personality</Label>
                      <Input
                        value={editor.personality}
                        onChange={(event) =>
                          setAgentEditor((current) =>
                            current
                              ? {
                                  ...current,
                                  personality: event.currentTarget.value,
                                }
                              : current
                          )
                        }
                      />
                    </div>
                  </div>

                  <div className="grid gap-3 md:grid-cols-2">
                    <div className="space-y-1.5">
                      <Label>Provider</Label>
                      <Input
                        value={editor.provider}
                        onChange={(event) =>
                          setAgentEditor((current) =>
                            current
                              ? {
                                  ...current,
                                  provider: event.currentTarget.value,
                                }
                              : current
                          )
                        }
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>Model</Label>
                      <Input
                        value={editor.model}
                        onChange={(event) =>
                          setAgentEditor((current) =>
                            current
                              ? {
                                  ...current,
                                  model: event.currentTarget.value,
                                }
                              : current
                          )
                        }
                      />
                    </div>
                  </div>

                  <div className="space-y-1.5">
                    <Label>Tools (comma separated)</Label>
                    <Input
                      value={editor.tools}
                      onChange={(event) =>
                        setAgentEditor((current) =>
                          current
                            ? {
                                ...current,
                                tools: event.currentTarget.value,
                              }
                            : current
                        )
                      }
                    />
                  </div>

                  <div className="space-y-1.5">
                    <Label>System prompt</Label>
                    <Textarea
                      value={editor.prompt}
                      onChange={(event) =>
                        setAgentEditor((current) =>
                          current
                            ? {
                                ...current,
                                prompt: event.currentTarget.value,
                              }
                            : current
                        )
                      }
                      rows={10}
                    />
                  </div>

                  <div className="flex justify-end">
                    <Button type="button" onClick={handleSaveAgentEditor}>
                      Save agent config
                    </Button>
                  </div>
                </div>
              ) : null}
            </section>

            <section className="rounded-2xl border bg-white/85 p-4">
              <p className="text-xs font-semibold uppercase tracking-[0.12em] text-zinc-500">
                Agent timeline
              </p>
              <div className="mt-2 space-y-2">
                {agent.activity.length === 0 ? (
                  <p className="text-xs text-zinc-500">No activity yet.</p>
                ) : (
                  [...agent.activity]
                    .sort((left, right) => right.at.localeCompare(left.at))
                    .map((event) => (
                      <article
                        key={event.id}
                        className="rounded-xl border px-3 py-2"
                      >
                        <div className="flex items-center justify-between gap-2">
                          <div className="flex items-center gap-2">
                            <Badge className={ACTIVITY_KIND_BADGE[event.kind]}>
                              {event.kind}
                            </Badge>
                            <p className="text-xs font-medium">
                              {event.summary}
                            </p>
                          </div>
                          <p className="text-[11px] text-zinc-500">
                            {formatDate(event.at)}
                          </p>
                        </div>
                        {event.detail ? (
                          <p className="mt-1 whitespace-pre-wrap text-xs text-zinc-600">
                            {event.detail}
                          </p>
                        ) : null}
                      </article>
                    ))
                )}
              </div>
            </section>
          </div>
        </ScrollArea>
      </div>
    );
  };

  return (
    <div className="devmode-shell relative h-screen w-screen overflow-hidden text-zinc-900">
      <div className="devmode-grid grid h-full w-full grid-cols-1 md:grid-cols-[4.5rem_18.5rem_1fr]">
        <aside className="border-border/70 flex items-center gap-2 border-b bg-zinc-950 px-2 py-2 text-zinc-200 md:flex-col md:items-center md:border-b-0 md:border-r md:px-0 md:py-3">
          <div className="flex items-center gap-2 md:flex-col md:gap-3">
            <div className="rounded-xl bg-sky-500/20 px-2 py-1 text-[11px] font-semibold tracking-[0.12em] text-sky-200">
              DM
            </div>
            {store.workspaces.map((workspace) => (
              <button
                key={workspace.id}
                type="button"
                title={workspace.name}
                onClick={() => selectWorkspace(workspace.id)}
                className={`size-10 rounded-2xl border text-sm font-semibold transition ${
                  workspace.id === store.selectedWorkspaceId
                    ? "border-sky-300/70 bg-sky-500/20 text-sky-100"
                    : "border-zinc-800 bg-zinc-900 text-zinc-300 hover:border-zinc-700 hover:bg-zinc-800"
                }`}
              >
                {workspace.icon}
              </button>
            ))}
          </div>

          <div className="md:mt-auto">
            <Button
              type="button"
              size="icon"
              variant="outline"
              className="border-zinc-700 bg-zinc-900 text-zinc-200 hover:bg-zinc-800"
              onClick={() => {
                setWorkspaceDraft(copyWorkspaceDraft(DEFAULT_WORKSPACE_DRAFT));
                setIsWorkspaceDialogOpen(true);
              }}
            >
              +
            </Button>
          </div>
        </aside>

        <aside className="border-border/70 flex min-h-0 flex-col border-b bg-white/75 md:border-b-0 md:border-r">
          {activeWorkspace ? (
            <>
              <header className="border-border/70 border-b px-4 py-4">
                <p className="text-[11px] uppercase tracking-[0.12em] text-zinc-500">
                  Workspace
                </p>
                <h2 className="text-base font-semibold">
                  {activeWorkspace.name}
                </h2>
                <p className="text-xs text-zinc-500">
                  {activeWorkspace.namespace}
                </p>
                <div className="mt-2 flex flex-wrap items-center gap-1">
                  <Badge
                    className={
                      activeWorkspace.scanner.status === "ready"
                        ? "bg-emerald-500/12 text-emerald-700"
                        : activeWorkspace.scanner.status === "running"
                          ? "bg-amber-500/12 text-amber-700"
                          : activeWorkspace.scanner.status === "error"
                            ? "bg-rose-500/12 text-rose-700"
                            : "bg-zinc-500/10 text-zinc-700"
                    }
                  >
                    scanner {activeWorkspace.scanner.status}
                  </Badge>
                  {activeWorkspace.scanner.lastScanAt ? (
                    <span className="text-[11px] text-zinc-500">
                      last run {formatDate(activeWorkspace.scanner.lastScanAt)}
                    </span>
                  ) : null}
                </div>
              </header>

              <ScrollArea className="min-h-0 flex-1">
                <div className="space-y-5 p-3">
                  <section>
                    <div className="mb-2 flex items-center justify-between">
                      <p className="text-[11px] uppercase tracking-[0.12em] text-zinc-500">
                        Tasks
                      </p>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => setIsTaskDialogOpen(true)}
                      >
                        New
                      </Button>
                    </div>

                    <div className="space-y-1">
                      <button
                        type="button"
                        onClick={() =>
                          setStore((previous) => ({
                            ...previous,
                            view: { type: "tasks" },
                          }))
                        }
                        className={`w-full rounded-xl px-2 py-1.5 text-left text-sm ${
                          store.view.type === "tasks"
                            ? "bg-sky-500/12 text-sky-700"
                            : "hover:bg-zinc-100"
                        }`}
                      >
                        All tasks ({activeWorkspace.tasks.length})
                      </button>
                      {activeWorkspace.tasks.map((task) => (
                        <button
                          key={task.id}
                          type="button"
                          onClick={() =>
                            setStore((previous) => ({
                              ...previous,
                              view: { type: "task", taskId: task.id },
                            }))
                          }
                          className={`w-full rounded-xl px-2 py-1.5 text-left text-sm ${
                            store.view.type === "task" &&
                            store.view.taskId === task.id
                              ? "bg-sky-500/12 text-sky-700"
                              : "hover:bg-zinc-100"
                          }`}
                        >
                          <p className="font-medium">{task.identifier}</p>
                          <p className="truncate text-xs text-zinc-500">
                            {task.title}
                          </p>
                        </button>
                      ))}
                    </div>
                  </section>

                  <section>
                    <div className="mb-2 flex items-center justify-between">
                      <p className="text-[11px] uppercase tracking-[0.12em] text-zinc-500">
                        Agents
                      </p>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => {
                          setAgentDraft({ ...DEFAULT_AGENT_DRAFT });
                          setIsAgentDialogOpen(true);
                        }}
                      >
                        Add
                      </Button>
                    </div>

                    <div className="space-y-1">
                      {activeWorkspace.agents.map((agent) => (
                        <button
                          key={agent.id}
                          type="button"
                          onClick={() =>
                            setStore((previous) => ({
                              ...previous,
                              view: { type: "agent", agentId: agent.id },
                            }))
                          }
                          className={`w-full rounded-xl px-2 py-1.5 text-left text-sm ${
                            store.view.type === "agent" &&
                            store.view.agentId === agent.id
                              ? "bg-sky-500/12 text-sky-700"
                              : "hover:bg-zinc-100"
                          }`}
                        >
                          <p className="font-medium">{agent.name}</p>
                          <p className="truncate text-xs text-zinc-500">
                            {agent.role} · {agent.provider}/{agent.model}
                          </p>
                        </button>
                      ))}
                    </div>
                  </section>

                  <section>
                    <p className="mb-2 text-[11px] uppercase tracking-[0.12em] text-zinc-500">
                      Scanner priorities
                    </p>
                    <div className="flex flex-wrap gap-1">
                      {activeWorkspace.scanner.priorities.map((entry) => (
                        <Badge key={entry} variant="outline">
                          {entry}
                        </Badge>
                      ))}
                    </div>
                  </section>
                </div>
              </ScrollArea>
            </>
          ) : (
            <div className="flex h-full items-center justify-center p-4 text-center text-sm text-zinc-500">
              Create your first workspace to start.
            </div>
          )}
        </aside>

        <main className="min-h-0 overflow-hidden bg-white/70">
          {!activeWorkspace ? (
            <div className="flex h-full items-center justify-center p-6">
              <div className="w-full max-w-2xl rounded-3xl border bg-white/85 p-8">
                <p className="text-[11px] uppercase tracking-[0.14em] text-zinc-500">
                  DevMode
                </p>
                <h1 className="mt-2 text-2xl font-semibold">
                  Local-first team workspace
                </h1>
                <p className="mt-3 text-sm text-zinc-600">
                  Create a workspace, set namespace and project root, then let
                  the scanner actor prioritize docs and prepare context for your
                  team.
                </p>
                <div className="mt-4 flex flex-wrap gap-2">
                  {DOC_SCAN_PRIORITIES.map((entry) => (
                    <Badge key={entry} variant="outline">
                      {entry}
                    </Badge>
                  ))}
                </div>
                <div className="mt-6">
                  <Button
                    type="button"
                    onClick={() => {
                      setWorkspaceDraft(
                        copyWorkspaceDraft(DEFAULT_WORKSPACE_DRAFT)
                      );
                      setIsWorkspaceDialogOpen(true);
                    }}
                  >
                    Create workspace
                  </Button>
                </div>
              </div>
            </div>
          ) : store.view.type === "tasks" ? (
            renderTasksView(activeWorkspace)
          ) : store.view.type === "task" && selectedTask ? (
            renderTaskDetail(activeWorkspace, selectedTask)
          ) : store.view.type === "agent" && selectedAgent ? (
            renderAgentDetail(activeWorkspace, selectedAgent)
          ) : (
            <div className="flex h-full items-center justify-center text-sm text-zinc-500">
              Select a workspace item.
            </div>
          )}
        </main>
      </div>

      <Dialog
        open={isWorkspaceDialogOpen}
        onOpenChange={setIsWorkspaceDialogOpen}
      >
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Create Workspace</DialogTitle>
            <DialogDescription>
              Each workspace is isolated. The scanner actor will prioritize
              README, ARCHITECTURE, HACKING, docs, RFCs, and RFDs.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label>Name</Label>
              <Input
                value={workspaceDraft.name}
                onChange={(event) =>
                  setWorkspaceDraft((previous) => ({
                    ...previous,
                    name: event.currentTarget.value,
                  }))
                }
                placeholder="Borg Core"
              />
            </div>

            <div className="grid gap-3 md:grid-cols-2">
              <div className="space-y-1.5">
                <Label>Namespace</Label>
                <Input
                  value={workspaceDraft.namespace}
                  onChange={(event) =>
                    setWorkspaceDraft((previous) => ({
                      ...previous,
                      namespace: event.currentTarget.value,
                    }))
                  }
                  placeholder="borg:*"
                />
              </div>

              <div className="space-y-1.5">
                <Label>Project root</Label>
                <Input
                  value={workspaceDraft.projectRoot}
                  onChange={(event) =>
                    setWorkspaceDraft((previous) => ({
                      ...previous,
                      projectRoot: event.currentTarget.value,
                    }))
                  }
                  placeholder="/Users/you/Developer/project"
                />
              </div>
            </div>

            <div className="rounded-xl border bg-zinc-50 p-3 text-xs text-zinc-600">
              Runtime sync target:{" "}
              <code>{resolveDefaultBaseUrl() || "(same-origin)"}</code>
            </div>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setIsWorkspaceDialogOpen(false)}
            >
              Cancel
            </Button>
            <Button type="button" onClick={handleCreateWorkspace}>
              Create workspace
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={isTaskDialogOpen} onOpenChange={setIsTaskDialogOpen}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Create Task</DialogTitle>
            <DialogDescription>
              New tasks trigger PM/Designer/Engineer reviews with suggested
              subtasks.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label>Title</Label>
              <Input
                value={taskDraft.title}
                onChange={(event) =>
                  setTaskDraft((previous) => ({
                    ...previous,
                    title: event.currentTarget.value,
                  }))
                }
                placeholder="Implement workspace docs scanner activity feed"
              />
            </div>

            <div className="space-y-1.5">
              <Label>Description</Label>
              <Textarea
                value={taskDraft.description}
                onChange={(event) =>
                  setTaskDraft((previous) => ({
                    ...previous,
                    description: event.currentTarget.value,
                  }))
                }
                rows={6}
                placeholder="What needs to happen, constraints, and expected output"
              />
            </div>

            <div className="space-y-1.5">
              <Label>Labels</Label>
              <Input
                value={taskDraft.labels}
                onChange={(event) =>
                  setTaskDraft((previous) => ({
                    ...previous,
                    labels: event.currentTarget.value,
                  }))
                }
                placeholder="frontend, local-first, planning"
              />
            </div>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setIsTaskDialogOpen(false)}
            >
              Cancel
            </Button>
            <Button type="button" onClick={handleCreateTask}>
              Create task
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={isAgentDialogOpen} onOpenChange={setIsAgentDialogOpen}>
        <DialogContent className="max-w-3xl">
          <DialogHeader>
            <DialogTitle>Add Agent</DialogTitle>
            <DialogDescription>
              Configure prompt, model, and tools. Agent profiles are synced to
              Borg GraphQL runtime.
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-3 md:grid-cols-2">
            <div className="space-y-1.5">
              <Label>Name</Label>
              <Input
                value={agentDraft.name}
                onChange={(event) =>
                  setAgentDraft((previous) => ({
                    ...previous,
                    name: event.currentTarget.value,
                  }))
                }
                placeholder="Reliability Engineer"
              />
            </div>

            <div className="space-y-1.5">
              <Label>Role</Label>
              <Select
                value={agentDraft.role}
                onValueChange={(value) =>
                  setAgentDraft((previous) => ({
                    ...previous,
                    role: normalizeRole(value),
                  }))
                }
              >
                <SelectTrigger>
                  <SelectValue placeholder="Role" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="Product Manager">
                    Product Manager
                  </SelectItem>
                  <SelectItem value="Designer">Designer</SelectItem>
                  <SelectItem value="Engineer">Engineer</SelectItem>
                  <SelectItem value="Custom">Custom</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-1.5">
              <Label>Provider</Label>
              <Input
                value={agentDraft.provider}
                onChange={(event) =>
                  setAgentDraft((previous) => ({
                    ...previous,
                    provider: event.currentTarget.value,
                  }))
                }
                placeholder="openai"
              />
            </div>

            <div className="space-y-1.5">
              <Label>Model</Label>
              <Input
                value={agentDraft.model}
                onChange={(event) =>
                  setAgentDraft((previous) => ({
                    ...previous,
                    model: event.currentTarget.value,
                  }))
                }
                placeholder="gpt-5"
              />
            </div>

            <div className="space-y-1.5 md:col-span-2">
              <Label>Personality</Label>
              <Input
                value={agentDraft.personality}
                onChange={(event) =>
                  setAgentDraft((previous) => ({
                    ...previous,
                    personality: event.currentTarget.value,
                  }))
                }
                placeholder="Calm and exacting"
              />
            </div>

            <div className="space-y-1.5 md:col-span-2">
              <Label>Tools (comma separated)</Label>
              <Input
                value={agentDraft.tools}
                onChange={(event) =>
                  setAgentDraft((previous) => ({
                    ...previous,
                    tools: event.currentTarget.value,
                  }))
                }
                placeholder="repo.search, repo.edit, terminal.exec"
              />
            </div>

            <div className="space-y-1.5 md:col-span-2">
              <Label>Prompt</Label>
              <Textarea
                value={agentDraft.prompt}
                onChange={(event) =>
                  setAgentDraft((previous) => ({
                    ...previous,
                    prompt: event.currentTarget.value,
                  }))
                }
                rows={8}
                placeholder="Describe goals, guardrails, and decision policy."
              />
            </div>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setIsAgentDialogOpen(false)}
            >
              Cancel
            </Button>
            <Button type="button" onClick={handleCreateAgent}>
              Add agent
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {notice ? (
        <div className="fixed bottom-5 left-1/2 z-50 w-[min(92vw,42rem)] -translate-x-1/2 rounded-xl border bg-white px-3 py-2 text-sm shadow-lg">
          {notice}
        </div>
      ) : null}
    </div>
  );
}
