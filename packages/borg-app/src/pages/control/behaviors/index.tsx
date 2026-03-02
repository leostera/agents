import { type BehaviorRecord, createBorgApiClient } from "@borg/api";
import {
  Badge,
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
import { Brain, LoaderCircle, Pause, Play, Plus, Trash2 } from "lucide-react";
import React from "react";
import {
  Section,
  SectionContent,
  SectionEmpty,
  SectionToolbar,
} from "../../../components/Section";
import {
  AddBehaviorForm,
  type AddBehaviorInput,
} from "./AddBehaviorForm";

const borgApi = createBorgApiClient();

export function BehaviorsPage() {
  const [behaviors, setBehaviors] = React.useState<BehaviorRecord[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [providers, setProviders] = React.useState<
    { provider: string; enabled: boolean }[]
  >([]);
  const [isDialogOpen, setIsDialogOpen] = React.useState(false);
  const [isSaving, setIsSaving] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);

  const loadBehaviors = React.useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [behaviorRows, providerRows] = await Promise.all([
        borgApi.listBehaviors(500),
        borgApi.listProviders(500),
      ]);
      setBehaviors(behaviorRows);
      setProviders(providerRows);
    } catch (loadError) {
      setBehaviors([]);
      setProviders([]);
      setError(
        loadError instanceof Error
          ? loadError.message
          : "Unable to load behaviors"
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void loadBehaviors();
  }, [loadBehaviors]);

  const filteredBehaviors = React.useMemo(() => {
    const term = query.trim().toLowerCase();
    if (!term) return behaviors;
    return behaviors.filter((behavior) =>
      [
        behavior.behavior_id,
        behavior.name,
        behavior.status,
        behavior.system_prompt,
      ]
        .join(" ")
        .toLowerCase()
        .includes(term)
    );
  }, [behaviors, query]);

  const hasNoBehaviors = !isLoading && behaviors.length === 0;

  const handleCreateBehavior = async (input: AddBehaviorInput) => {
    setError(null);
    setIsSaving(true);
    try {
      await borgApi.upsertBehavior({
        behaviorId: input.behaviorId,
        name: input.name,
        systemPrompt: input.systemPrompt,
        preferredProviderId: input.preferredProviderId ?? null,
        sessionTurnConcurrency: "serial",
        requiredCapabilitiesJson: [],
        status: "ACTIVE",
      });
      setIsDialogOpen(false);
      await loadBehaviors();
    } catch (saveError) {
      setError(
        saveError instanceof Error
          ? saveError.message
          : "Unable to create behavior"
      );
    } finally {
      setIsSaving(false);
    }
  };

  const handleDeleteBehavior = async (behaviorId: string) => {
    setError(null);
    try {
      await borgApi.deleteBehavior(behaviorId);
      await loadBehaviors();
    } catch (deleteError) {
      setError(
        deleteError instanceof Error
          ? deleteError.message
          : "Unable to delete behavior"
      );
    }
  };

  const handleSetStatus = async (behavior: BehaviorRecord, status: string) => {
    setError(null);
    try {
      await borgApi.upsertBehavior({
        behaviorId: behavior.behavior_id,
        name: behavior.name,
        systemPrompt: behavior.system_prompt,
        preferredProviderId: behavior.preferred_provider_id ?? null,
        requiredCapabilitiesJson: behavior.required_capabilities_json,
        sessionTurnConcurrency: behavior.session_turn_concurrency,
        status,
      });
      await loadBehaviors();
    } catch (statusError) {
      setError(
        statusError instanceof Error
          ? statusError.message
          : "Unable to update behavior status"
      );
    }
  };

  return (
    <Section className="gap-4">
      {hasNoBehaviors ? null : (
        <SectionToolbar>
          <Input
            value={query}
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder="Search behaviors by id, name, status, or prompt"
            aria-label="Search behaviors"
            className="max-w-md"
          />
          <Button variant="outline" onClick={() => setIsDialogOpen(true)}>
            <Plus className="size-4" />
            Add Behavior
          </Button>
        </SectionToolbar>
      )}

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <SectionContent>
        {hasNoBehaviors ? (
          <SectionEmpty
            icon={Brain}
            title="No Behaviors Found"
            description="Create your first behavior to define actor policy."
            action={
              <Button onClick={() => setIsDialogOpen(true)}>
                + Add Behavior
              </Button>
            }
          />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Status</TableHead>
                <TableHead>Behavior</TableHead>
                <TableHead>Provider</TableHead>
                <TableHead>Updated</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading ? (
                <TableRow>
                  <TableCell colSpan={5} className="text-muted-foreground text-center">
                    <span className="inline-flex items-center gap-2">
                      <LoaderCircle className="size-4 animate-spin" />
                      Loading behaviors...
                    </span>
                  </TableCell>
                </TableRow>
              ) : (
                filteredBehaviors.map((behavior) => (
                  <TableRow key={behavior.behavior_id}>
                    <TableCell>
                      <Badge
                        className={
                          behavior.status === "ACTIVE"
                            ? "border-emerald-300 bg-emerald-100 text-emerald-900"
                            : "border-rose-300 bg-rose-100 text-rose-900"
                        }
                      >
                        {behavior.status === "ACTIVE" ? "Active" : "Inactive"}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <EntityLink
                        uri={behavior.behavior_id}
                        name={behavior.name}
                        className="inline-flex items-center gap-1"
                      />
                    </TableCell>
                    <TableCell>{behavior.preferred_provider_id ?? "—"}</TableCell>
                    <TableCell>
                      {new Date(behavior.updated_at).toLocaleString()}
                    </TableCell>
                    <TableCell className="space-x-2">
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={() => void handleSetStatus(behavior, "ACTIVE")}
                        disabled={behavior.status === "ACTIVE"}
                        title="Activate behavior"
                        aria-label={`Activate ${behavior.name}`}
                      >
                        <Play className="size-3.5" />
                      </Button>
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={() => void handleSetStatus(behavior, "INACTIVE")}
                        disabled={behavior.status !== "ACTIVE"}
                        title="Deactivate behavior"
                        aria-label={`Deactivate ${behavior.name}`}
                      >
                        <Pause className="size-3.5" />
                      </Button>
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={() => void handleDeleteBehavior(behavior.behavior_id)}
                        aria-label={`Delete ${behavior.behavior_id}`}
                        title="Delete behavior"
                      >
                        <Trash2 className="size-3.5" />
                      </Button>
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        )}
      </SectionContent>

      <AddBehaviorForm
        providers={providers}
        open={isDialogOpen}
        onOpenChange={setIsDialogOpen}
        isSaving={isSaving}
        onSubmit={handleCreateBehavior}
      />
    </Section>
  );
}
