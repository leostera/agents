import { type AgentSpecRecord, createBorgApiClient } from "@borg/api";
import {
  Button,
  EntityLink,
  Input,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@borg/ui";
import { Bot, LoaderCircle, Pencil, Plus, Power } from "lucide-react";
import React from "react";
import {
  Section,
  SectionContent,
  SectionEmpty,
  SectionToolbar,
} from "../../../components/Section";
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
  const hasNoAgents = !isLoading && agents.length === 0;

  const handleCreateAgent = async (input: {
    agentId: string;
    name: string;
    provider: string;
    model: string;
    systemPrompt: string;
  }) => {
    setError(null);

    setIsSaving(true);
    try {
      await borgApi.upsertAgentSpec({
        agentId: input.agentId,
        name: input.name,
        defaultProviderId: input.provider,
        model: input.model.trim(),
        systemPrompt: input.systemPrompt,
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

  const navigateToAgentDetails = React.useCallback((agentId: string) => {
    window.history.pushState(null, "", `/control/agents/${agentId}`);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }, []);

  const handleToggleAgentEnabled = async (
    agentId: string,
    enabled: boolean
  ) => {
    setError(null);
    try {
      await borgApi.setAgentSpecEnabled(agentId, !enabled);
      await loadAgents();
    } catch (toggleError) {
      setError(
        toggleError instanceof Error
          ? toggleError.message
          : "Unable to update agent status"
      );
    }
  };

  return (
    <Section className="gap-4">
      {hasNoAgents ? null : (
        <SectionToolbar>
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
        </SectionToolbar>
      )}

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <SectionContent>
        {hasNoAgents ? (
          <SectionEmpty
            icon={Bot}
            title="No Agents Found"
            description="Create your first agent to configure model, tools, and behavior."
            action={
              <Button onClick={() => setIsDialogOpen(true)}>+ Add Agent</Button>
            }
          />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-[44px]">Status</TableHead>
                <TableHead>Agent</TableHead>
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
                  <TableRow
                    key={agent.agent_id}
                    className="cursor-pointer"
                    onClick={() => navigateToAgentDetails(agent.agent_id)}
                  >
                    <TableCell>
                      <span
                        className={
                          agent.enabled
                            ? "inline-block size-2.5 rounded-full bg-emerald-500"
                            : "inline-block size-2.5 rounded-full bg-rose-500"
                        }
                      />
                    </TableCell>
                    <TableCell>
                      <EntityLink
                        uri={agent.agent_id}
                        name={agent.name || "Agent"}
                        className="inline-flex items-center gap-1"
                      />
                    </TableCell>
                    <TableCell>{agent.model}</TableCell>
                    <TableCell>
                      {new Date(agent.updated_at).toLocaleString()}
                    </TableCell>
                    <TableCell className="space-x-2">
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={(event) => {
                          event.stopPropagation();
                          navigateToAgentDetails(agent.agent_id);
                        }}
                        aria-label={`Edit ${agent.name || agent.agent_id}`}
                        title="Edit agent"
                      >
                        <Pencil className="size-3.5" />
                      </Button>
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={(event) => {
                          event.stopPropagation();
                          void handleToggleAgentEnabled(
                            agent.agent_id,
                            agent.enabled
                          );
                        }}
                        aria-label={`${agent.enabled ? "Disable" : "Enable"} ${agent.agent_id}`}
                        title={agent.enabled ? "Disable agent" : "Enable agent"}
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
      </SectionContent>

      <AddAgentForm
        open={isDialogOpen}
        onOpenChange={setIsDialogOpen}
        isSaving={isSaving}
        onSubmit={handleCreateAgent}
      />
    </Section>
  );
}
