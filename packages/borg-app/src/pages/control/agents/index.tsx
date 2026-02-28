import { type AgentSpecRecord, createBorgApiClient } from "@borg/api";
import {
  Button,
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
  Input,
  Link,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@borg/ui";
import { Bot, LoaderCircle, Plus, Power } from "lucide-react";
import React from "react";
import { AddAgentForm } from "./AddAgentForm";

const borgApi = createBorgApiClient();

export function AgentsPage() {
  const [agents, setAgents] = React.useState<AgentSpecRecord[]>([]);
  const [query, setQuery] = React.useState(
    () => new URLSearchParams(window.location.search).get("q") ?? ""
  );
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [isDialogOpen, setIsDialogOpen] = React.useState(false);
  const [isSaving, setIsSaving] = React.useState(false);

  const loadAgents = React.useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const rows = await borgApi.listAgentSpecs(500);
      setAgents(rows);
    } catch (loadError) {
      setAgents([]);
      setError(
        loadError instanceof Error ? loadError.message : "Unable to load agents"
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  React.useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (query.trim()) {
      params.set("q", query.trim());
    } else {
      params.delete("q");
    }
    const paramsString = params.toString();
    const url = paramsString
      ? `/control/agents?${paramsString}`
      : "/control/agents";
    window.history.replaceState(null, "", url);
  }, [query]);

  const filteredAgents = React.useMemo(() => {
    const term = query.trim().toLowerCase();
    if (!term) return agents;
    return agents.filter((agent) => {
      const haystack = [agent.agent_id, agent.model, agent.system_prompt]
        .join(" ")
        .toLowerCase();
      return haystack.includes(term);
    });
  }, [agents, query]);

  const handleCreateAgent = async (input: {
    agentId: string;
    name: string;
    provider: string;
    model: string;
    systemPrompt: string;
    tools: unknown;
  }) => {
    setError(null);

    setIsSaving(true);
    try {
      await borgApi.upsertAgentSpec({
        agentId: input.agentId,
        name: input.name,
        model: input.model.trim(),
        systemPrompt: input.systemPrompt,
        tools: input.tools,
      });
      setIsDialogOpen(false);
      await loadAgents();
    } catch (saveError) {
      setError(
        saveError instanceof Error
          ? saveError.message
          : "Unable to create agent"
      );
    } finally {
      setIsSaving(false);
    }
  };

  const handleDisableAgent = async (agentId: string) => {
    setError(null);
    try {
      await borgApi.deleteAgentSpec(agentId, { ignoreNotFound: true });
      await loadAgents();
    } catch (deleteError) {
      setError(
        deleteError instanceof Error
          ? deleteError.message
          : "Unable to disable agent"
      );
    }
  };

  return (
    <section className="space-y-4">
      <section className="flex flex-wrap items-center gap-2">
        <Input
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          placeholder="Search agents by id, model, or prompt"
          aria-label="Search agents"
          className="max-w-md"
        />
        <Button variant="outline" onClick={() => setIsDialogOpen(true)}>
          <Plus className="size-4" />
          Add Agent
        </Button>
      </section>

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      {!isLoading && filteredAgents.length === 0 ? (
        <Empty className="border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Bot />
            </EmptyMedia>
            <EmptyTitle>No Agents Found</EmptyTitle>
            <EmptyDescription>
              Create your first agent to configure model, tools, and behavior.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent className="flex-row justify-center">
            <Button onClick={() => setIsDialogOpen(true)}>+ Add Agent</Button>
          </EmptyContent>
        </Empty>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Agent</TableHead>
              <TableHead>Name</TableHead>
              <TableHead>Model</TableHead>
              <TableHead>Updated</TableHead>
              <TableHead>Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading ? (
              <TableRow>
                <TableCell
                  colSpan={5}
                  className="text-muted-foreground text-center"
                >
                  <span className="inline-flex items-center gap-2">
                    <LoaderCircle className="size-4 animate-spin" />
                    Loading agents...
                  </span>
                </TableCell>
              </TableRow>
            ) : (
              filteredAgents.map((agent) => (
                <TableRow key={agent.agent_id}>
                  <TableCell className="font-mono text-[11px]">
                    <Link href={`/control/agents/${agent.agent_id}`}>
                      {agent.agent_id}
                    </Link>
                  </TableCell>
                  <TableCell>{agent.name || "Agent"}</TableCell>
                  <TableCell>{agent.model}</TableCell>
                  <TableCell>
                    {new Date(agent.updated_at).toLocaleString()}
                  </TableCell>
                  <TableCell>
                    <Button
                      size="icon-sm"
                      variant="outline"
                      onClick={() => void handleDisableAgent(agent.agent_id)}
                      aria-label={`Disable ${agent.agent_id}`}
                      title="Disable agent"
                    >
                      <Power className="size-3.5" />
                    </Button>
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      )}

      <AddAgentForm
        open={isDialogOpen}
        onOpenChange={setIsDialogOpen}
        isSaving={isSaving}
        onSubmit={handleCreateAgent}
      />
    </section>
  );
}
