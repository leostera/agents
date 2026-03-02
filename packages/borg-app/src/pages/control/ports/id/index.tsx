import {
  type ActorRecord,
  createBorgApiClient,
  type PortActorBinding,
  type PortBinding,
  type PortRecord,
  type SessionRecord,
} from "@borg/api";
import {
  Badge,
  Button,
  Input,
  Link,
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
} from "@borg/ui";
import React from "react";
import { resolvePortFromRoute } from "../utils";

const borgApi = createBorgApiClient();
const NO_ACTOR = "__none__";

type PortDetailsPageProps = {
  portUri: string;
};

function normalizeTimestamp(value?: string | null): string {
  if (!value) return "—";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return "—";
  return parsed.toLocaleString();
}

function toAllowedExternalIds(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value
    .map((entry) => (typeof entry === "string" ? entry.trim() : ""))
    .filter(Boolean);
}

function readableMode(allowsGuests: boolean): string {
  return allowsGuests ? "Public" : "Private";
}

function providerKind(provider: string): string {
  return provider.trim().toLowerCase();
}

function toMemoryEntityHref(uri: string): string {
  return `/memory/entity/${encodeURIComponent(uri)}`;
}

function isValidTelegramAllowedUserId(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) return false;
  if (/^\d+$/.test(trimmed)) return true;
  return /^@[a-zA-Z0-9_]{5,32}$/.test(trimmed);
}

function isValidDiscordAllowedUserId(value: string): boolean {
  const trimmed = value.trim();
  return /^[0-9]{16,21}$/.test(trimmed);
}

export function PortDetailsPage({ portUri }: PortDetailsPageProps) {
  const [port, setPort] = React.useState<PortRecord | null>(null);
  const [actors, setActors] = React.useState<ActorRecord[]>([]);
  const [bindings, setBindings] = React.useState<PortBinding[]>([]);
  const [actorBindings, setActorBindings] = React.useState<
    Record<string, string>
  >({});
  const [actorSelection, setActorSelection] = React.useState<
    Record<string, string>
  >({});
  const [sessions, setSessions] = React.useState<SessionRecord[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [isSaving, setIsSaving] = React.useState(false);

  const [enabled, setEnabled] = React.useState(true);
  const [mode, setMode] = React.useState<"public" | "private">("public");
  const [assignedActorId, setAssignedActorId] = React.useState(NO_ACTOR);
  const [botToken, setBotToken] = React.useState("");
  const [allowedExternalUserIds, setAllowedExternalUserIds] = React.useState<
    string[]
  >([]);
  const [allowedUserInput, setAllowedUserInput] = React.useState("");

  const load = React.useCallback(async () => {
    const normalizedPortUri = portUri.trim();
    if (!normalizedPortUri) {
      setError("Missing port name");
      setPort(null);
      setBindings([]);
      setActorBindings({});
      setActorSelection({});
      setSessions([]);
      setActors([]);
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const [loadedPorts, loadedActors, loadedSessions] =
        await Promise.all([
          borgApi.listPorts(1000),
          borgApi.listActors(1000),
          borgApi.listSessions(10000),
        ]);

      const selectedPort = resolvePortFromRoute(normalizedPortUri, loadedPorts);
      if (!selectedPort) {
        throw new Error(`Port not found: ${normalizedPortUri}`);
      }

      const loadedBindings = await borgApi.listPortBindings(
        selectedPort.port_id,
        1000
      );
      const loadedActorBindings = await borgApi.listPortActorBindings(
        selectedPort.port_id,
        1000
      );
      const actorByConversation = loadedActorBindings.reduce<
        Record<string, string>
      >((acc, item: PortActorBinding) => {
        if (item.actor_id) {
          acc[item.conversation_key] = item.actor_id;
        }
        return acc;
      }, {});

      setPort(selectedPort);
      setActors(loadedActors);
      setBindings(loadedBindings);
      setActorBindings(actorByConversation);
      setActorSelection(actorByConversation);
      setSessions(
        loadedSessions.filter((session) => session.port === normalizedPortUri)
      );

      const settings = selectedPort.settings ?? {};
      setEnabled(Boolean(selectedPort.enabled));
      setMode(selectedPort.allows_guests ? "public" : "private");
      setAssignedActorId(selectedPort.default_agent_id ?? NO_ACTOR);
      setBotToken(
        typeof settings.bot_token === "string" ? settings.bot_token : ""
      );
      setAllowedExternalUserIds(
        toAllowedExternalIds(settings.allowed_external_user_ids)
      );
      setAllowedUserInput("");
    } catch (loadError) {
      setPort(null);
      setBindings([]);
      setActorBindings({});
      setActorSelection({});
      setSessions([]);
      setActors([]);
      setError(
        loadError instanceof Error
          ? loadError.message
          : "Unable to load port details"
      );
    } finally {
      setIsLoading(false);
    }
  }, [portUri]);

  React.useEffect(() => {
    void load();
  }, [load]);

  const isTelegramPort = providerKind(port?.provider ?? "") === "telegram";
  const isDiscordPort = providerKind(port?.provider ?? "") === "discord";

  const addAllowedUser = React.useCallback(() => {
    const next = allowedUserInput.trim();
    if (!next) return;
    if (isTelegramPort && !isValidTelegramAllowedUserId(next)) {
      setError(
        "Allowed user must be a numeric Telegram ID (e.g. 2654566) or @username (e.g. @leostera)."
      );
      return;
    }
    if (isDiscordPort && !isValidDiscordAllowedUserId(next)) {
      setError("Allowed user must be a numeric Discord user ID (snowflake).");
      return;
    }
    setAllowedExternalUserIds((current) => {
      if (current.includes(next)) return current;
      return [...current, next];
    });
    setError(null);
    setAllowedUserInput("");
  }, [allowedUserInput, isDiscordPort, isTelegramPort]);

  const removeAllowedUser = React.useCallback((userId: string) => {
    setAllowedExternalUserIds((current) =>
      current.filter((value) => value !== userId)
    );
  }, []);

  const handleSavePort = React.useCallback(
    async (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      if (!port) return;

      setIsSaving(true);
      setError(null);
      try {
        const nextSettings: Record<string, unknown> = {
          ...(port.settings ?? {}),
        };
        if (isTelegramPort || isDiscordPort) {
          nextSettings.bot_token = botToken.trim();
          nextSettings.allowed_external_user_ids = allowedExternalUserIds;
        }
        await borgApi.upsertPort(port.port_id, {
          provider: port.provider,
          enabled,
          allows_guests: mode === "public",
          default_agent_id: assignedActorId === NO_ACTOR ? null : assignedActorId,
          settings: nextSettings,
        });
        await load();
      } catch (saveError) {
        setError(
          saveError instanceof Error ? saveError.message : "Unable to save port"
        );
      } finally {
        setIsSaving(false);
      }
    },
    [
      allowedExternalUserIds,
      assignedActorId,
      botToken,
      enabled,
      isDiscordPort,
      isTelegramPort,
      load,
      mode,
      port,
    ]
  );

  const handleActorBindingSave = React.useCallback(
    async (conversationKey: string) => {
      if (!port) return;
      const selectedActor = actorSelection[conversationKey] ?? NO_ACTOR;
      setError(null);
      try {
        if (selectedActor === NO_ACTOR) {
          await borgApi.deletePortActorBinding(port.port_id, conversationKey, {
            ignoreNotFound: true,
          });
        } else {
          await borgApi.upsertPortActorBinding(
            port.port_id,
            conversationKey,
            selectedActor
          );
        }
        await load();
      } catch (saveError) {
        setError(
          saveError instanceof Error
            ? saveError.message
            : "Unable to save actor binding"
        );
      }
    },
    [actorSelection, load, port]
  );

  if (isLoading) {
    return <p className="text-muted-foreground text-sm">Loading port...</p>;
  }

  if (!port) {
    return (
      <p className="text-destructive text-sm">{error ?? "Port not found."}</p>
    );
  }

  return (
    <section className="space-y-6">
      {error ? <p className="text-destructive text-sm">{error}</p> : null}

      <section className="grid gap-3 md:grid-cols-4">
        <div>
          <p className="text-muted-foreground text-xs">Port</p>
          <p className="font-mono text-xs break-all">{port.port_id}</p>
        </div>
        <div>
          <p className="text-muted-foreground text-xs">Updated</p>
          <p className="text-xs">{normalizeTimestamp(port.updated_at)}</p>
        </div>
      </section>

      <section className="space-y-2">
        <p className="text-sm font-semibold">Edit Port</p>
        <form className="space-y-3" onSubmit={handleSavePort}>
          <div className="grid gap-2 md:grid-cols-3">
            <div className="space-y-1">
              <p className="text-muted-foreground text-xs">Status</p>
              <Button
                type="button"
                variant="outline"
                onClick={() => setEnabled((current) => !current)}
              >
                {enabled ? "Disable" : "Enable"}
              </Button>
            </div>
            <div className="space-y-1">
              <p className="text-muted-foreground text-xs">Mode</p>
              <Select
                value={mode}
                onValueChange={(value) =>
                  setMode(value === "private" ? "private" : "public")
                }
              >
                <SelectTrigger>
                  <SelectValue placeholder="Select mode" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="public">Public (allows guests)</SelectItem>
                  <SelectItem value="private">
                    Private (does not allow guests)
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <p className="text-muted-foreground text-xs">Assigned Actor</p>
              <Select value={assignedActorId} onValueChange={setAssignedActorId}>
                <SelectTrigger>
                  <SelectValue placeholder="Select actor" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={NO_ACTOR}>No assigned actor</SelectItem>
                  {actors.map((actor) => (
                    <SelectItem key={actor.actor_id} value={actor.actor_id}>
                      {actor.name || actor.actor_id}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>

          {isTelegramPort || isDiscordPort ? (
            <section className="space-y-3 rounded-md border p-3">
              <p className="text-sm font-semibold">
                {isTelegramPort ? "Telegram Details" : "Discord Details"}
              </p>
              <div className="space-y-1">
                <p className="text-muted-foreground text-xs">bot_token</p>
                <Input
                  type="password"
                  value={botToken}
                  onChange={(event) => setBotToken(event.currentTarget.value)}
                  placeholder="Telegram bot token"
                  aria-label="Bot token"
                />
              </div>

              {mode === "private" ? (
                <div className="space-y-2">
                  <p className="text-muted-foreground text-xs">
                    allowed_external_user_ids
                  </p>
                  <p className="text-muted-foreground text-[11px]">
                    {isTelegramPort
                      ? "Use numeric Telegram IDs (for example 2654566) or usernames (for example @leostera)."
                      : "Use numeric Discord user IDs (snowflakes)."}
                  </p>
                  <div className="flex items-center gap-2">
                    <Input
                      value={allowedUserInput}
                      onChange={(event) =>
                        setAllowedUserInput(event.currentTarget.value)
                      }
                      placeholder={
                        isTelegramPort
                          ? "2654566 or @leostera"
                          : "123456789012345678"
                      }
                      aria-label="Allowed user id"
                    />
                    <Button
                      type="button"
                      variant="outline"
                      onClick={addAllowedUser}
                    >
                      + Add
                    </Button>
                  </div>
                  {allowedExternalUserIds.length === 0 ? (
                    <p className="text-muted-foreground text-xs">
                      No allowed users configured.
                    </p>
                  ) : (
                    <div className="space-y-1">
                      {allowedExternalUserIds.map((userId) => (
                        <div
                          key={userId}
                          className="flex items-center justify-between rounded border px-2 py-1"
                        >
                          <span className="font-mono text-xs">{userId}</span>
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => removeAllowedUser(userId)}
                          >
                            Remove
                          </Button>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              ) : null}
            </section>
          ) : null}

          <Button type="submit" disabled={isSaving}>
            {isSaving ? "Saving..." : "Save"}
          </Button>
        </form>
      </section>

      <section className="space-y-2">
        <p className="text-sm font-semibold">Conversation Bindings</p>
        <p className="text-muted-foreground text-xs">
          A binding maps an external conversation key (for example a Telegram
          chat id) to a Borg session and optional actor binding.
        </p>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Conversation</TableHead>
              <TableHead>Session</TableHead>
              <TableHead>Agent</TableHead>
              <TableHead>Actor</TableHead>
              <TableHead>Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {bindings.length === 0 ? (
              <TableRow>
                <TableCell
                  colSpan={5}
                  className="text-muted-foreground text-center"
                >
                  No bindings.
                </TableCell>
              </TableRow>
            ) : (
              bindings.map((binding) => (
                <TableRow key={binding.conversation_key}>
                  <TableCell className="font-mono text-[11px]">
                    <Link href={toMemoryEntityHref(binding.conversation_key)}>
                      {binding.conversation_key}
                    </Link>
                  </TableCell>
                  <TableCell className="font-mono text-[11px]">
                    <Link href={toMemoryEntityHref(binding.session_id)}>
                      {binding.session_id}
                    </Link>
                  </TableCell>
                  <TableCell className="font-mono text-[11px]">
                    {binding.agent_id ? (
                      <Link href={toMemoryEntityHref(binding.agent_id)}>
                        {binding.agent_id}
                      </Link>
                    ) : (
                      "—"
                    )}
                  </TableCell>
                  <TableCell>
                    <Select
                      value={
                        actorSelection[binding.conversation_key] ??
                        actorBindings[binding.conversation_key] ??
                        NO_ACTOR
                      }
                      onValueChange={(value) =>
                        setActorSelection((current) => ({
                          ...current,
                          [binding.conversation_key]: value,
                        }))
                      }
                    >
                      <SelectTrigger className="max-w-[26rem] font-mono text-[11px]">
                        <SelectValue placeholder="No actor binding" />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value={NO_ACTOR}>No actor binding</SelectItem>
                        {actors.map((actor) => (
                          <SelectItem key={actor.actor_id} value={actor.actor_id}>
                            {actor.name} ({actor.actor_id})
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </TableCell>
                  <TableCell className="space-x-2">
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={() =>
                        void handleActorBindingSave(binding.conversation_key)
                      }
                    >
                      Save
                    </Button>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={() => {
                        setActorSelection((current) => ({
                          ...current,
                          [binding.conversation_key]: NO_ACTOR,
                        }));
                        void handleActorBindingSave(binding.conversation_key);
                      }}
                    >
                      Clear
                    </Button>
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </section>

      <section className="space-y-2">
        <p className="text-sm font-semibold">Active Sessions</p>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Session ID</TableHead>
              <TableHead>Users</TableHead>
              <TableHead>Updated</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {sessions.length === 0 ? (
              <TableRow>
                <TableCell
                  colSpan={3}
                  className="text-muted-foreground text-center"
                >
                  No active sessions.
                </TableCell>
              </TableRow>
            ) : (
              sessions.map((session) => (
                <TableRow key={session.session_id}>
                  <TableCell className="font-mono text-[11px]">
                    {session.session_id}
                  </TableCell>
                  <TableCell className="font-mono text-[11px]">
                    {session.users.join(", ")}
                  </TableCell>
                  <TableCell>
                    {normalizeTimestamp(session.updated_at)}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </section>
    </section>
  );
}
