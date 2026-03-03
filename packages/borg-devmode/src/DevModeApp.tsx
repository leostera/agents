import {
  type ActorRecord,
  type BehaviorRecord,
  createBorgApiClient,
} from "@borg/api";
import {
  Button,
  ChatComposerShell,
  ChatThread,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Textarea,
} from "@borg/ui";
import { Bot, ClipboardList, LoaderCircle, Settings } from "lucide-react";
import React from "react";

const borgApi = createBorgApiClient();

const PLANNING_SESSION_ID = "borg:session:devmode:planning";
const PLANNING_USER_ID = "borg:user:devmode";
const PLANNING_PORT_ID = "borg:port:devmode";
const DEV_MODE_PLANNER_ACTOR_ID = "devmode:actor:planner";
const DEV_MODE_PLANNER_NAME = "DevMode Planner";
const DEV_MODE_PLANNER_PROMPT =
  "You are DevMode's planning lead. Help the user refine implementation-ready specs, ask focused clarifying questions, and break approved specs into parallelizable task-graph work.";

type ChatMessage = {
  id: string;
  role: "assistant" | "user" | "system";
  text: string;
  timestamp: string;
  pending?: boolean;
};

function formatDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function extractText(payload: Record<string, unknown>): string {
  if (typeof payload.content === "string" && payload.content.trim()) {
    return payload.content;
  }
  if (typeof payload.text === "string" && payload.text.trim()) {
    return payload.text;
  }
  if (payload.type === "tool_call") {
    const name = typeof payload.name === "string" ? payload.name : "tool";
    return `Tool call: ${name}`;
  }
  if (payload.type === "tool_result") {
    const name = typeof payload.name === "string" ? payload.name : "tool";
    return `Tool result: ${name}`;
  }
  try {
    return JSON.stringify(payload, null, 2);
  } catch {
    return String(payload);
  }
}

function detectRole(payload: Record<string, unknown>): ChatMessage["role"] {
  if (typeof payload.type === "string") {
    const type = payload.type.trim().toLowerCase();
    if (type === "assistant") return "assistant";
    if (type === "user") return "user";
    if (type === "system") return "system";
    if (
      type === "tool_call" ||
      type === "tool_result" ||
      type === "session_event"
    ) {
      return "system";
    }
  }
  if (typeof payload.role === "string") {
    const role = payload.role.trim().toLowerCase();
    if (role === "assistant" || role === "agent") return "assistant";
    if (role === "user") return "user";
  }
  return "system";
}

function isChatPayload(payload: Record<string, unknown>): boolean {
  const typeCandidate = payload.type;
  if (typeof typeCandidate === "string") {
    const type = typeCandidate.trim().toLowerCase();
    if (type === "user" || type === "assistant") return true;
    if (type === "system") return false;
    if (
      type === "tool_call" ||
      type === "tool_result" ||
      type === "session_event"
    )
      return false;
  }

  const roleCandidate =
    typeof payload.role === "string"
      ? payload.role.trim().toLowerCase()
      : typeof payload.author === "string"
        ? payload.author.trim().toLowerCase()
        : null;
  if (roleCandidate) {
    if (
      roleCandidate === "assistant" ||
      roleCandidate === "agent" ||
      roleCandidate === "user"
    ) {
      return true;
    }
    return false;
  }

  return (
    typeof payload.content === "string" || typeof payload.text === "string"
  );
}

function toChatMessages(rawMessages: Record<string, unknown>[]): ChatMessage[] {
  const seen = new Set<string>();
  const mapped = rawMessages
    .filter((raw) => isChatPayload(raw as Record<string, unknown>))
    .map((raw, index) => {
      const payload = raw as Record<string, unknown>;
      const rawTimestamp =
        typeof payload.created_at === "string"
          ? payload.created_at
          : typeof payload.timestamp === "string"
            ? payload.timestamp
            : typeof payload.updated_at === "string"
              ? payload.updated_at
              : null;
      const role = detectRole(payload);
      const text = extractText(payload);
      const timestamp = rawTimestamp
        ? formatDate(rawTimestamp)
        : nowTimestamp();
      const messageIdentity =
        (typeof payload.message_id === "string" && payload.message_id.trim()) ||
        `${role}|${text}|${timestamp}`;
      return {
        id: `planning-message-${index}`,
        role,
        text,
        timestamp,
        messageIdentity,
      };
    });

  return mapped
    .filter((message) => {
      if (seen.has(message.messageIdentity)) {
        return false;
      }
      seen.add(message.messageIdentity);
      return true;
    })
    .map(({ messageIdentity: _messageIdentity, ...message }) => message);
}

function nowTimestamp(): string {
  return new Date().toLocaleString();
}

export function DevModeApp() {
  const [actors, setActors] = React.useState<ActorRecord[]>([]);
  const [messages, setMessages] = React.useState<ChatMessage[]>([]);
  const [draft, setDraft] = React.useState("");
  const [isLoading, setIsLoading] = React.useState(true);
  const [isSending, setIsSending] = React.useState(false);
  const [isSavingSettings, setIsSavingSettings] = React.useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = React.useState(false);
  const [plannerPromptDraft, setPlannerPromptDraft] = React.useState(
    DEV_MODE_PLANNER_PROMPT
  );
  const [error, setError] = React.useState<string | null>(null);
  const sendInFlightRef = React.useRef(false);
  const plannerActor = React.useMemo(
    () =>
      actors.find((actor) => actor.actor_id === DEV_MODE_PLANNER_ACTOR_ID) ??
      null,
    [actors]
  );

  React.useEffect(() => {
    if (plannerActor) {
      setPlannerPromptDraft(plannerActor.system_prompt);
    }
  }, [plannerActor]);

  const resolvePlannerBehavior = React.useCallback(
    (behaviors: BehaviorRecord[]): string | null => {
      if (behaviors.length === 0) return null;
      if (
        behaviors.some(
          (behavior) => behavior.behavior_id === "borg:behavior:default"
        )
      ) {
        return "borg:behavior:default";
      }
      const active = behaviors.find(
        (behavior) => behavior.status.trim().toUpperCase() === "ACTIVE"
      );
      return active?.behavior_id ?? behaviors[0].behavior_id;
    },
    []
  );

  const ensurePlannerActor = React.useCallback(
    async (actorRows: ActorRecord[]): Promise<ActorRecord[]> => {
      if (
        actorRows.some((actor) => actor.actor_id === DEV_MODE_PLANNER_ACTOR_ID)
      ) {
        return actorRows;
      }
      const behaviors = await borgApi.listBehaviors(500);
      const defaultBehaviorId = resolvePlannerBehavior(behaviors);
      if (!defaultBehaviorId) {
        throw new Error(
          "No behaviors available to create devmode:actor:planner. Create a behavior first."
        );
      }
      await borgApi.upsertActor({
        actorId: DEV_MODE_PLANNER_ACTOR_ID,
        name: DEV_MODE_PLANNER_NAME,
        systemPrompt: DEV_MODE_PLANNER_PROMPT,
        defaultBehaviorId,
        status: "RUNNING",
      });
      return await borgApi.listActors(500);
    },
    [resolvePlannerBehavior]
  );

  const loadAll = React.useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [actorRows, sessionRows] = await Promise.all([
        borgApi.listActors(500),
        borgApi.listSessionMessages(PLANNING_SESSION_ID, {
          from: 0,
          limit: 500,
        }),
      ]);
      const withPlanner = await ensurePlannerActor(actorRows);
      setActors(withPlanner);
      setMessages(toChatMessages(sessionRows));
    } catch (loadError) {
      setError(
        loadError instanceof Error
          ? loadError.message
          : "Unable to load planning view"
      );
    } finally {
      setIsLoading(false);
    }
  }, [ensurePlannerActor]);

  React.useEffect(() => {
    void loadAll();
  }, [loadAll]);

  const submitMessage = React.useCallback(async () => {
    const text = draft.trim();
    if (!text || sendInFlightRef.current) return;
    sendInFlightRef.current = true;

    const optimisticId =
      typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
        ? `optimistic-${crypto.randomUUID()}`
        : `optimistic-${Date.now()}`;

    setMessages((previous) => [
      ...previous,
      {
        id: optimisticId,
        role: "user",
        text,
        timestamp: nowTimestamp(),
        pending: true,
      },
    ]);
    setDraft("");
    setIsSending(true);
    setError(null);

    try {
      await borgApi.postHttpPort({
        userKey: PLANNING_USER_ID,
        text,
        sessionId: PLANNING_SESSION_ID,
        actorId: DEV_MODE_PLANNER_ACTOR_ID,
        metadata: {
          port: PLANNING_PORT_ID,
          channel: "planning",
        },
      });

      const sessionRows = await borgApi.listSessionMessages(
        PLANNING_SESSION_ID,
        {
          from: 0,
          limit: 500,
        }
      );
      setMessages(toChatMessages(sessionRows));
    } catch (sendError) {
      setMessages((previous) => [
        ...previous.map((message) =>
          message.id === optimisticId ? { ...message, pending: false } : message
        ),
        {
          id: `error-${optimisticId}`,
          role: "system",
          text: "Failed to send message. Try again.",
          timestamp: nowTimestamp(),
        },
      ]);
      setError(
        sendError instanceof Error
          ? sendError.message
          : "Unable to send planning message"
      );
    } finally {
      setIsSending(false);
      sendInFlightRef.current = false;
    }
  }, [draft]);

  const savePlannerSettings = React.useCallback(async () => {
    if (!plannerActor) return;
    const prompt = plannerPromptDraft.trim();
    if (!prompt) {
      setError("Planner Actor Prompt cannot be empty.");
      return;
    }

    setIsSavingSettings(true);
    setError(null);
    try {
      await borgApi.upsertActor({
        actorId: plannerActor.actor_id,
        name: plannerActor.name,
        systemPrompt: prompt,
        defaultBehaviorId: plannerActor.default_behavior_id,
        status: plannerActor.status,
      });
      setActors((previous) =>
        previous.map((actor) =>
          actor.actor_id === plannerActor.actor_id
            ? { ...actor, system_prompt: prompt }
            : actor
        )
      );
      setIsSettingsOpen(false);
    } catch (saveError) {
      setError(
        saveError instanceof Error
          ? saveError.message
          : "Unable to save planner settings"
      );
    } finally {
      setIsSavingSettings(false);
    }
  }, [plannerActor, plannerPromptDraft]);

  return (
    <div className="bg-background text-foreground flex min-h-screen">
      <aside className="w-64 border-r bg-muted/20 p-4">
        <div className="rounded-xl border bg-background p-3 shadow-sm">
          <p className="text-muted-foreground text-xs uppercase tracking-[0.12em]">
            Borg
          </p>
          <p className="text-sm font-semibold">DevMode</p>
        </div>

        <nav className="mt-4 space-y-1">
          <button
            type="button"
            className="bg-primary/10 text-primary flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm font-medium"
          >
            <ClipboardList className="size-4" />
            Planning
          </button>
        </nav>
      </aside>

      <main className="min-w-0 flex-1 p-5">
        <div className="h-[calc(100vh-2.5rem)] min-h-[620px]">
          <section className="flex h-full min-h-0 flex-col rounded-2xl border bg-card">
            <header className="space-y-3 border-b px-4 py-3">
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                  <Bot className="size-4" />
                  <p className="text-sm font-semibold">Planning Session</p>
                </div>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  onClick={() => setIsSettingsOpen(true)}
                  aria-label="Planning settings"
                >
                  <Settings className="size-4" />
                </Button>
              </div>
            </header>

            <div className="min-h-0 flex-1">
              <ChatThread
                messages={messages}
                isLoading={isSending}
                emptyTitle="No planning messages yet"
                emptyDescription="Start by describing the initiative you want to plan."
              >
                <ChatComposerShell
                  value={draft}
                  onChange={setDraft}
                  onSubmit={() => void submitMessage()}
                  isRunning={isSending}
                  placeholder="Describe the feature, constraint, or outcome you want to plan..."
                />
              </ChatThread>
            </div>

            {actors.length === 0 ? (
              <p className="text-muted-foreground border-t px-4 py-2 text-xs">
                Loading planner actor...
              </p>
            ) : null}
          </section>
        </div>

        {isLoading ? (
          <div className="pointer-events-none fixed bottom-5 right-5 rounded-full border bg-card p-2 shadow">
            <LoaderCircle className="size-4 animate-spin" />
          </div>
        ) : null}

        {error ? (
          <div className="bg-destructive/10 text-destructive fixed bottom-5 left-1/2 z-20 -translate-x-1/2 rounded-md border px-3 py-2 text-sm">
            {error}
          </div>
        ) : null}

        <Dialog open={isSettingsOpen} onOpenChange={setIsSettingsOpen}>
          <DialogContent className="flex max-h-[90vh] w-[min(96vw,72rem)] max-w-6xl flex-col overflow-hidden">
            <DialogHeader>
              <DialogTitle>Planning Settings</DialogTitle>
              <DialogDescription>
                Configure how the `devmode:actor:planner` behaves in this
                planning session.
              </DialogDescription>
            </DialogHeader>
            <div className="min-h-0 flex-1 space-y-2 overflow-y-auto pr-1">
              <p className="text-sm font-medium">Planner Actor Prompt</p>
              <Textarea
                value={plannerPromptDraft}
                onChange={(event) =>
                  setPlannerPromptDraft(event.currentTarget.value)
                }
                rows={24}
                className="min-h-[28rem] w-full resize-y overflow-y-auto"
                placeholder="Enter planner actor prompt..."
              />
            </div>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => setIsSettingsOpen(false)}
                disabled={isSavingSettings}
              >
                Cancel
              </Button>
              <Button
                type="button"
                onClick={() => void savePlannerSettings()}
                disabled={isSavingSettings || !plannerActor}
              >
                {isSavingSettings ? "Saving..." : "Save"}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </main>
    </div>
  );
}
