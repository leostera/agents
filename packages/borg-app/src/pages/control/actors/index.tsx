import { type ActorRecord, createBorgApiClient } from "@borg/api";
import { Badge, Button, EntityLink, Input, Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@borg/ui";
import { Bot, LoaderCircle, Pause, Play, Plus, Trash2 } from "lucide-react";
import React from "react";
import { Section, SectionContent, SectionEmpty, SectionToolbar } from "../../../components/Section";
import { AddActorForm, type AddActorInput } from "./AddActorForm";

const borgApi = createBorgApiClient();

export function ActorsPage() {
  const [actors, setActors] = React.useState<ActorRecord[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [isDialogOpen, setIsDialogOpen] = React.useState(false);
  const [isSaving, setIsSaving] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);

  const loadActors = React.useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const rows = await borgApi.listActors(500);
      setActors(rows);
    } catch (loadError) {
      setActors([]);
      setError(loadError instanceof Error ? loadError.message : "Unable to load actors");
    } finally {
      setIsLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void loadActors();
  }, [loadActors]);

  const filteredActors = React.useMemo(() => {
    const term = query.trim().toLowerCase();
    if (!term) return actors;
    return actors.filter((actor) =>
      [actor.actor_id, actor.name, actor.status, actor.system_prompt]
        .join(" ")
        .toLowerCase()
        .includes(term)
    );
  }, [actors, query]);

  const hasNoActors = !isLoading && actors.length === 0;

  const handleCreateActor = async (input: AddActorInput) => {
    setError(null);
    setIsSaving(true);
    try {
      await borgApi.upsertActor({
        actorId: input.actorId,
        name: input.name,
        systemPrompt: input.systemPrompt,
        status: "STOPPED",
      });
      setIsDialogOpen(false);
      await loadActors();
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "Unable to create actor");
    } finally {
      setIsSaving(false);
    }
  };

  const handleDeleteActor = async (actorId: string) => {
    setError(null);
    try {
      await borgApi.deleteActor(actorId);
      await loadActors();
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "Unable to delete actor");
    }
  };

  const handleSetActorStatus = async (actor: ActorRecord, status: string) => {
    setError(null);
    try {
      await borgApi.upsertActor({
        actorId: actor.actor_id,
        name: actor.name,
        systemPrompt: actor.system_prompt,
        status,
      });
      await loadActors();
    } catch (statusError) {
      setError(
        statusError instanceof Error
          ? statusError.message
          : "Unable to update actor status"
      );
    }
  };

  return (
    <Section className="gap-4">
      {hasNoActors ? null : (
        <SectionToolbar>
          <Input
            value={query}
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder="Search actors by id, name, status, or prompt"
            aria-label="Search actors"
            className="max-w-md"
          />
          <Button variant="outline" onClick={() => setIsDialogOpen(true)}>
            <Plus className="size-4" />
            Add Actor
          </Button>
        </SectionToolbar>
      )}

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <SectionContent>
        {hasNoActors ? (
          <SectionEmpty
            icon={Bot}
            title="No Actors Found"
            description="Create an actor so ports can bind conversations to it."
            action={<Button onClick={() => setIsDialogOpen(true)}>+ Add Actor</Button>}
          />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Status</TableHead>
                <TableHead>Actor</TableHead>
                <TableHead>Updated</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading ? (
                <TableRow>
                  <TableCell colSpan={4} className="text-muted-foreground text-center">
                    <span className="inline-flex items-center gap-2">
                      <LoaderCircle className="size-4 animate-spin" />
                      Loading actors...
                    </span>
                  </TableCell>
                </TableRow>
              ) : (
                filteredActors.map((actor) => (
                  <TableRow key={actor.actor_id}>
                    <TableCell>
                      <Badge
                        className={
                          actor.status === "RUNNING"
                            ? "border-emerald-300 bg-emerald-100 text-emerald-900"
                            : "border-rose-300 bg-rose-100 text-rose-900"
                        }
                      >
                        {actor.status === "RUNNING" ? "Running" : "Stopped"}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <EntityLink uri={actor.actor_id} name={actor.name} className="inline-flex items-center gap-1" />
                    </TableCell>
                    <TableCell>{new Date(actor.updated_at).toLocaleString()}</TableCell>
                    <TableCell className="space-x-2">
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={() => void handleSetActorStatus(actor, "RUNNING")}
                        disabled={actor.status === "RUNNING"}
                        title="Start actor"
                        aria-label={`Start ${actor.name}`}
                      >
                        <Play className="size-3.5" />
                      </Button>
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={() => void handleSetActorStatus(actor, "STOPPED")}
                        disabled={actor.status === "STOPPED"}
                        title="Stop actor"
                        aria-label={`Stop ${actor.name}`}
                      >
                        <Pause className="size-3.5" />
                      </Button>
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={() => void handleDeleteActor(actor.actor_id)}
                        aria-label={`Delete ${actor.actor_id}`}
                        title="Delete actor"
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

      <AddActorForm
        open={isDialogOpen}
        onOpenChange={setIsDialogOpen}
        isSaving={isSaving}
        onSubmit={handleCreateActor}
      />
    </Section>
  );
}
