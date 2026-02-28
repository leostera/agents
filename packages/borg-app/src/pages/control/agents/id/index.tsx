import {
  type AgentSpecRecord,
  createBorgApiClient,
  type ProviderRecord,
  type SessionRecord,
} from "@borg/api";
import {
  Badge,
  Button,
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
  Input,
  Label,
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
  Textarea,
} from "@borg/ui";
import { LoaderCircle, Power, Save } from "lucide-react";
import React from "react";

const borgApi = createBorgApiClient();

type AgentDetailsPageProps = {
  agentId: string;
};

type AgentFormState = {
  name: string;
  provider: string;
  model: string;
  systemPrompt: string;
};

type SessionMessageRow = {
  sessionId: string;
  index: number;
  snippet: string;
};

function stringifyMessage(message: Record<string, unknown>): string {
  const content =
    typeof message.content === "string"
      ? message.content
      : typeof message.text === "string"
        ? message.text
        : JSON.stringify(message);
  return content.length > 180 ? `${content.slice(0, 180)}…` : content;
}

function containsAgentId(value: unknown, agentId: string): boolean {
  if (typeof value === "string") {
    return value.includes(agentId);
  }
  if (Array.isArray(value)) {
    return value.some((entry) => containsAgentId(entry, agentId));
  }
  if (value && typeof value === "object") {
    return Object.values(value).some((entry) =>
      containsAgentId(entry, agentId)
    );
  }
  return false;
}

export function AgentDetailsPage({ agentId }: AgentDetailsPageProps) {
  const [agent, setAgent] = React.useState<AgentSpecRecord | null>(null);
  const [form, setForm] = React.useState<AgentFormState>({
    name: "",
    provider: "",
    model: "",
    systemPrompt: "",
  });
  const [providers, setProviders] = React.useState<ProviderRecord[]>([]);
  const [models, setModels] = React.useState<string[]>([]);
  const [isLoadingProviders, setIsLoadingProviders] = React.useState(false);
  const [isLoadingModels, setIsLoadingModels] = React.useState(false);
  const [sessions, setSessions] = React.useState<SessionRecord[]>([]);
  const [messages, setMessages] = React.useState<SessionMessageRow[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [isSaving, setIsSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const loadModelsForProvider = React.useCallback(
    async (provider: string, preferredModel?: string) => {
      if (!provider) {
        setModels([]);
        setForm((current) => ({ ...current, model: "" }));
        return;
      }
      setIsLoadingModels(true);
      try {
        const rows = await borgApi.listProviderModels(provider);
        setModels(rows);
        setForm((current) => ({
          ...current,
          model:
            preferredModel && rows.includes(preferredModel)
              ? preferredModel
              : (rows[0] ?? ""),
        }));
      } catch (loadError) {
        setModels([]);
        setForm((current) => ({ ...current, model: "" }));
        setError(
          loadError instanceof Error
            ? loadError.message
            : "Unable to load provider models"
        );
      } finally {
        setIsLoadingModels(false);
      }
    },
    []
  );

  const loadAgent = React.useCallback(async () => {
    if (!agentId.trim()) {
      setAgent(null);
      setError("Missing agent id");
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setIsLoadingProviders(true);
    setError(null);
    try {
      const [spec, recentSessions, providerRows] = await Promise.all([
        borgApi.getAgentSpec(agentId),
        borgApi.listSessions(200),
        borgApi.listProviders(100),
      ]);

      if (!spec) {
        setAgent(null);
        setError("Agent not found");
        return;
      }

      setAgent(spec);
      setProviders(providerRows);
      const resolvedProvider =
        providerRows.find(
          (provider) => provider.provider === spec.default_provider_id
        )?.provider ??
        providerRows[0]?.provider ??
        "";
      setForm({
        name: spec.name || "Agent",
        provider: resolvedProvider,
        model: "",
        systemPrompt: spec.system_prompt,
      });
      if (resolvedProvider) {
        await loadModelsForProvider(resolvedProvider, spec.model);
      } else {
        setModels([]);
        setForm((current) => ({ ...current, model: spec.model }));
      }

      const sampledSessions = recentSessions.slice(0, 20);
      const messageResults = await Promise.all(
        sampledSessions.map(async (session) => {
          try {
            const rows = await borgApi.listSessionMessages(session.session_id, {
              from: 0,
              limit: 60,
            });
            return { session, rows };
          } catch {
            return { session, rows: [] };
          }
        })
      );

      const matchedSessions: SessionRecord[] = [];
      const matchedMessages: SessionMessageRow[] = [];
      for (const result of messageResults) {
        const relevant = result.rows
          .map((row, index) => ({ row, index }))
          .filter(({ row }) => containsAgentId(row, agentId));
        if (relevant.length > 0) {
          matchedSessions.push(result.session);
          for (const entry of relevant) {
            if (typeof entry.row === "object" && entry.row !== null) {
              matchedMessages.push({
                sessionId: result.session.session_id,
                index: entry.index,
                snippet: stringifyMessage(entry.row as Record<string, unknown>),
              });
            }
          }
        }
      }

      setSessions(matchedSessions.slice(0, 10));
      setMessages(matchedMessages.slice(0, 20));
    } catch (loadError) {
      setAgent(null);
      setProviders([]);
      setModels([]);
      setSessions([]);
      setMessages([]);
      setError(
        loadError instanceof Error ? loadError.message : "Unable to load agent"
      );
    } finally {
      setIsLoadingProviders(false);
      setIsLoading(false);
    }
  }, [agentId, loadModelsForProvider]);

  React.useEffect(() => {
    void loadAgent();
  }, [loadAgent]);

  const handleSave = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);
    if (!form.provider) {
      setError("Select a provider");
      return;
    }
    if (!form.model) {
      setError("Select a model");
      return;
    }

    setIsSaving(true);
    try {
      await borgApi.upsertAgentSpec({
        agentId,
        name: form.name.trim() || "Agent",
        defaultProviderId: form.provider,
        model: form.model.trim(),
        systemPrompt: form.systemPrompt,
      });
      await loadAgent();
    } catch (saveError) {
      setError(
        saveError instanceof Error ? saveError.message : "Unable to save agent"
      );
    } finally {
      setIsSaving(false);
    }
  };

  const handleDisable = async () => {
    setError(null);
    try {
      if (!agent) return;
      await borgApi.setAgentSpecEnabled(agentId, !agent.enabled);
      await loadAgent();
    } catch (toggleError) {
      setError(
        toggleError instanceof Error
          ? toggleError.message
          : "Unable to update agent status"
      );
    }
  };

  if (isLoading) {
    return (
      <p className="text-muted-foreground inline-flex items-center gap-2 text-sm">
        <LoaderCircle className="size-4 animate-spin" />
        Loading agent...
      </p>
    );
  }

  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_360px]">
      <section className="space-y-3">
        <section className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Badge variant="outline" className="font-mono text-[11px]">
              {agentId}
            </Badge>
            {agent ? (
              <Badge variant={agent.enabled ? "secondary" : "outline"}>
                {agent.enabled ? "Enabled" : "Disabled"}
              </Badge>
            ) : null}
          </div>
          <Button variant="outline" onClick={() => void handleDisable()}>
            <Power className="size-4" />
            {agent?.enabled ? "Disable" : "Enable"}
          </Button>
        </section>

        {error ? <p className="text-destructive text-xs">{error}</p> : null}

        {agent ? (
          <form className="space-y-3" onSubmit={handleSave}>
            <div className="space-y-1">
              <Label htmlFor="agent-name">Name</Label>
              <Input
                id="agent-name"
                value={form.name}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    name: event.currentTarget.value,
                  }))
                }
                required
              />
            </div>
            <div className="space-y-1">
              <Label>Provider</Label>
              <Select
                value={form.provider}
                onValueChange={(provider) => {
                  setError(null);
                  setForm((current) => ({
                    ...current,
                    provider,
                    model: "",
                  }));
                  void loadModelsForProvider(provider);
                }}
              >
                <SelectTrigger disabled={isLoadingProviders}>
                  <SelectValue
                    placeholder={
                      isLoadingProviders
                        ? "Loading providers..."
                        : "Select provider"
                    }
                  />
                </SelectTrigger>
                <SelectContent>
                  {providers.map((provider) => (
                    <SelectItem
                      key={provider.provider}
                      value={provider.provider}
                    >
                      {provider.provider}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label>Model</Label>
              <Combobox
                items={models}
                selectedValue={form.model || null}
                onSelectedValueChange={(value) =>
                  setForm((current) => ({ ...current, model: value ?? "" }))
                }
              >
                <ComboboxInput
                  disabled={!form.provider || isLoadingModels}
                  placeholder={
                    isLoadingModels ? "Loading models..." : "Select model"
                  }
                  showClear
                />
                <ComboboxContent>
                  <ComboboxEmpty>No models found.</ComboboxEmpty>
                  <ComboboxList>
                    {(model) => (
                      <ComboboxItem key={model} value={model}>
                        {model}
                      </ComboboxItem>
                    )}
                  </ComboboxList>
                </ComboboxContent>
              </Combobox>
            </div>
            <div className="space-y-1">
              <Label htmlFor="agent-system-prompt">System Prompt</Label>
              <Textarea
                id="agent-system-prompt"
                value={form.systemPrompt}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    systemPrompt: event.currentTarget.value,
                  }))
                }
                rows={8}
              />
            </div>
            <Button
              type="submit"
              disabled={isSaving || !form.provider || !form.model}
            >
              {isSaving ? (
                <LoaderCircle className="size-4 animate-spin" />
              ) : (
                <Save className="size-4" />
              )}
              Save Changes
            </Button>
          </form>
        ) : (
          <p className="text-muted-foreground text-sm">Agent not found.</p>
        )}
      </section>

      <aside className="space-y-4 rounded-lg border p-3">
        <section className="space-y-2">
          <p className="text-sm font-semibold">Recent Sessions</p>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Session</TableHead>
                <TableHead>Updated</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {sessions.length === 0 ? (
                <TableRow>
                  <TableCell
                    colSpan={2}
                    className="text-muted-foreground text-xs"
                  >
                    No recent sessions found.
                  </TableCell>
                </TableRow>
              ) : (
                sessions.map((session) => (
                  <TableRow key={session.session_id}>
                    <TableCell className="font-mono text-[11px]">
                      <Link href={`/control/sessions/${session.session_id}`}>
                        {session.session_id}
                      </Link>
                    </TableCell>
                    <TableCell className="text-xs">
                      {new Date(session.updated_at).toLocaleString()}
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </section>

        <section className="space-y-2">
          <p className="text-sm font-semibold">Recent Messages</p>
          <div className="space-y-2">
            {messages.length === 0 ? (
              <p className="text-muted-foreground text-xs">
                No recent messages found.
              </p>
            ) : (
              messages.map((message) => (
                <div
                  key={`${message.sessionId}:${message.index}`}
                  className="rounded border p-2"
                >
                  <p className="font-mono text-[10px] text-muted-foreground">
                    {message.sessionId} #{message.index}
                  </p>
                  <p className="text-xs">{message.snippet}</p>
                </div>
              ))
            )}
          </div>
        </section>
      </aside>
    </section>
  );
}
