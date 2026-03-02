import {
  type ActorRecord,
  type BehaviorRecord,
  createBorgApiClient,
} from "@borg/api";
import {
  Badge,
  Button,
  Input,
  Label,
  Link,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Textarea,
} from "@borg/ui";
import { ChevronLeft, Save, Trash2 } from "lucide-react";
import React from "react";

const borgApi = createBorgApiClient();

type ActorDetailsPageProps = {
  actorId: string;
};

const NO_BEHAVIOR = "__none__";

export function ActorDetailsPage({ actorId }: ActorDetailsPageProps) {
  const [actor, setActor] = React.useState<ActorRecord | null>(null);
  const [behaviors, setBehaviors] = React.useState<BehaviorRecord[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [isSaving, setIsSaving] = React.useState(false);
  const [isDeleting, setIsDeleting] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const [name, setName] = React.useState("");
  const [systemPrompt, setSystemPrompt] = React.useState("");
  const [status, setStatus] = React.useState("STOPPED");
  const [defaultBehaviorId, setDefaultBehaviorId] = React.useState("");

  const load = React.useCallback(async () => {
    const normalizedActorId = actorId.trim();
    if (!normalizedActorId) {
      setError("Missing actor id");
      setActor(null);
      setBehaviors([]);
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const [loadedActor, loadedBehaviors] = await Promise.all([
        borgApi.getActor(normalizedActorId),
        borgApi.listBehaviors(500),
      ]);
      if (!loadedActor) {
        setActor(null);
        setBehaviors(loadedBehaviors);
        setError("Actor not found");
        return;
      }

      setActor(loadedActor);
      setBehaviors(loadedBehaviors);
      setName(loadedActor.name);
      setSystemPrompt(loadedActor.system_prompt);
      setStatus(loadedActor.status);
      setDefaultBehaviorId(loadedActor.default_behavior_id);
    } catch (loadError) {
      setActor(null);
      setBehaviors([]);
      setError(
        loadError instanceof Error
          ? loadError.message
          : "Unable to load actor details"
      );
    } finally {
      setIsLoading(false);
    }
  }, [actorId]);

  React.useEffect(() => {
    void load();
  }, [load]);

  const activeBehaviors = React.useMemo(
    () => behaviors.filter((behavior) => behavior.status === "ACTIVE"),
    [behaviors]
  );

  const handleSave = React.useCallback(
    async (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      if (!actor) return;
      if (!defaultBehaviorId || defaultBehaviorId === NO_BEHAVIOR) {
        setError("Default behavior is required");
        return;
      }

      setIsSaving(true);
      setError(null);
      try {
        await borgApi.upsertActor({
          actorId: actor.actor_id,
          name: name.trim(),
          systemPrompt: systemPrompt.trim(),
          defaultBehaviorId,
          status,
        });
        await load();
      } catch (saveError) {
        setError(
          saveError instanceof Error ? saveError.message : "Unable to save actor"
        );
      } finally {
        setIsSaving(false);
      }
    },
    [actor, defaultBehaviorId, load, name, status, systemPrompt]
  );

  const handleDelete = React.useCallback(async () => {
    if (!actor) return;
    const shouldDelete = window.confirm(
      `Delete actor \"${actor.name}\" (${actor.actor_id})?`
    );
    if (!shouldDelete) return;

    setIsDeleting(true);
    setError(null);
    try {
      await borgApi.deleteActor(actor.actor_id, { ignoreNotFound: true });
      window.history.pushState(null, "", "/control/actors");
      window.dispatchEvent(new PopStateEvent("popstate"));
    } catch (deleteError) {
      setError(
        deleteError instanceof Error
          ? deleteError.message
          : "Unable to delete actor"
      );
      setIsDeleting(false);
    }
  }, [actor]);

  if (isLoading) {
    return <p className="text-muted-foreground text-sm">Loading actor...</p>;
  }

  if (!actor) {
    return <p className="text-destructive text-sm">{error ?? "Actor not found."}</p>;
  }

  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2">
        <Button variant="outline" size="sm" asChild>
          <Link href="/control/actors">
            <ChevronLeft className="size-4" />
            Back
          </Link>
        </Button>
        <h2 className="text-base font-semibold">Edit Actor</h2>
        <Badge
          className={
            actor.status === "RUNNING"
              ? "border-emerald-300 bg-emerald-100 text-emerald-900"
              : "border-rose-300 bg-rose-100 text-rose-900"
          }
        >
          {actor.status}
        </Badge>
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <div>
          <p className="text-muted-foreground text-xs">Actor URI</p>
          <p className="font-mono text-xs break-all">{actor.actor_id}</p>
        </div>
        <div>
          <p className="text-muted-foreground text-xs">Updated</p>
          <p className="text-xs">{new Date(actor.updated_at).toLocaleString()}</p>
        </div>
      </div>

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <form className="space-y-3" onSubmit={handleSave}>
        <div className="space-y-1">
          <Label htmlFor="actor-name">Name</Label>
          <Input
            id="actor-name"
            value={name}
            onChange={(event) => setName(event.currentTarget.value)}
          />
        </div>

        <div className="space-y-1">
          <Label htmlFor="actor-system-prompt">System Prompt</Label>
          <Textarea
            id="actor-system-prompt"
            rows={8}
            value={systemPrompt}
            onChange={(event) => setSystemPrompt(event.currentTarget.value)}
          />
        </div>

        <div className="grid gap-3 md:grid-cols-2">
          <div className="space-y-1">
            <Label>Status</Label>
            <Select value={status} onValueChange={setStatus}>
              <SelectTrigger>
                <SelectValue placeholder="Select status" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="RUNNING">RUNNING</SelectItem>
                <SelectItem value="STOPPED">STOPPED</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-1">
            <Label>Default Behavior</Label>
            <Select
              value={defaultBehaviorId || NO_BEHAVIOR}
              onValueChange={(value) => setDefaultBehaviorId(value)}
            >
              <SelectTrigger>
                <SelectValue placeholder="Select default behavior" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={NO_BEHAVIOR} disabled>
                  Select default behavior
                </SelectItem>
                {activeBehaviors.map((behavior) => (
                  <SelectItem key={behavior.behavior_id} value={behavior.behavior_id}>
                    {behavior.name} ({behavior.behavior_id})
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <Button type="submit" disabled={isSaving}>
            <Save className="size-4" />
            {isSaving ? "Saving..." : "Save Actor"}
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => void handleDelete()}
            disabled={isDeleting}
          >
            <Trash2 className="size-4" />
            {isDeleting ? "Deleting..." : "Delete Actor"}
          </Button>
        </div>
      </form>
    </section>
  );
}
