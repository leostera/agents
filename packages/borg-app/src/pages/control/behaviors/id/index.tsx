import {
  type BehaviorRecord,
  createBorgApiClient,
  type ProviderRecord,
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
import { ChevronLeft, Pause, Play, Save, Trash2 } from "lucide-react";
import React from "react";

const borgApi = createBorgApiClient();
const NO_PROVIDER = "__none__";

type BehaviorDetailsPageProps = {
  behaviorId: string;
};

export function BehaviorDetailsPage({ behaviorId }: BehaviorDetailsPageProps) {
  const [behavior, setBehavior] = React.useState<BehaviorRecord | null>(null);
  const [providers, setProviders] = React.useState<ProviderRecord[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [isSaving, setIsSaving] = React.useState(false);
  const [isDeleting, setIsDeleting] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const [name, setName] = React.useState("");
  const [systemPrompt, setSystemPrompt] = React.useState("");
  const [status, setStatus] = React.useState("ACTIVE");
  const [preferredProviderId, setPreferredProviderId] = React.useState("");

  const load = React.useCallback(async () => {
    const normalizedBehaviorId = behaviorId.trim();
    if (!normalizedBehaviorId) {
      setError("Missing behavior id");
      setBehavior(null);
      setProviders([]);
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const [loadedBehavior, loadedProviders] = await Promise.all([
        borgApi.getBehavior(normalizedBehaviorId),
        borgApi.listProviders(500),
      ]);
      if (!loadedBehavior) {
        setBehavior(null);
        setProviders(loadedProviders);
        setError("Behavior not found");
        return;
      }

      setBehavior(loadedBehavior);
      setProviders(loadedProviders);
      setName(loadedBehavior.name);
      setSystemPrompt(loadedBehavior.system_prompt);
      setStatus(loadedBehavior.status);
      setPreferredProviderId(loadedBehavior.preferred_provider_id ?? "");
    } catch (loadError) {
      setBehavior(null);
      setProviders([]);
      setError(
        loadError instanceof Error
          ? loadError.message
          : "Unable to load behavior details"
      );
    } finally {
      setIsLoading(false);
    }
  }, [behaviorId]);

  React.useEffect(() => {
    void load();
  }, [load]);

  const handleSave = React.useCallback(
    async (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      if (!behavior) return;

      setIsSaving(true);
      setError(null);
      try {
        await borgApi.upsertBehavior({
          behaviorId: behavior.behavior_id,
          name: name.trim(),
          systemPrompt: systemPrompt.trim(),
          preferredProviderId: preferredProviderId || null,
          requiredCapabilitiesJson: behavior.required_capabilities_json,
          sessionTurnConcurrency: "serial",
          status,
        });
        await load();
      } catch (saveError) {
        setError(
          saveError instanceof Error
            ? saveError.message
            : "Unable to save behavior"
        );
      } finally {
        setIsSaving(false);
      }
    },
    [behavior, load, name, preferredProviderId, status, systemPrompt]
  );

  const handleDelete = React.useCallback(async () => {
    if (!behavior) return;
    const shouldDelete = window.confirm(
      `Delete behavior \"${behavior.name}\" (${behavior.behavior_id})?`
    );
    if (!shouldDelete) return;

    setIsDeleting(true);
    setError(null);
    try {
      await borgApi.deleteBehavior(behavior.behavior_id, { ignoreNotFound: true });
      window.history.pushState(null, "", "/control/behaviors");
      window.dispatchEvent(new PopStateEvent("popstate"));
    } catch (deleteError) {
      setError(
        deleteError instanceof Error
          ? deleteError.message
          : "Unable to delete behavior"
      );
      setIsDeleting(false);
    }
  }, [behavior]);

  if (isLoading) {
    return <p className="text-muted-foreground text-sm">Loading behavior...</p>;
  }

  if (!behavior) {
    return <p className="text-destructive text-sm">{error ?? "Behavior not found."}</p>;
  }

  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2">
        <Button variant="outline" size="sm" asChild>
          <Link href="/control/behaviors">
            <ChevronLeft className="size-4" />
            Back
          </Link>
        </Button>
        <h2 className="text-base font-semibold">Edit Behavior</h2>
        <Badge
          className={
            behavior.status === "ACTIVE"
              ? "border-emerald-300 bg-emerald-100 text-emerald-900"
              : "border-rose-300 bg-rose-100 text-rose-900"
          }
        >
          {behavior.status}
        </Badge>
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <div>
          <p className="text-muted-foreground text-xs">Behavior URI</p>
          <p className="font-mono text-xs break-all">{behavior.behavior_id}</p>
        </div>
        <div>
          <p className="text-muted-foreground text-xs">Updated</p>
          <p className="text-xs">{new Date(behavior.updated_at).toLocaleString()}</p>
        </div>
      </div>

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <form className="space-y-3" onSubmit={handleSave}>
        <div className="space-y-1">
          <Label htmlFor="behavior-name">Name</Label>
          <Input
            id="behavior-name"
            value={name}
            onChange={(event) => setName(event.currentTarget.value)}
          />
        </div>

        <div className="space-y-1">
          <Label htmlFor="behavior-system-prompt">System Prompt</Label>
          <Textarea
            id="behavior-system-prompt"
            rows={8}
            value={systemPrompt}
            onChange={(event) => setSystemPrompt(event.currentTarget.value)}
          />
        </div>

        <div className="grid gap-3 md:grid-cols-2">
          <div className="space-y-1">
            <Label>Status</Label>
            <Button
              type="button"
              variant="outline"
              onClick={() =>
                setStatus((current) =>
                  current === "ACTIVE" ? "INACTIVE" : "ACTIVE"
                )
              }
            >
              {status === "ACTIVE" ? (
                <>
                  <Pause className="size-4" />
                  Stop
                </>
              ) : (
                <>
                  <Play className="size-4" />
                  Start
                </>
              )}
            </Button>
          </div>

          <div className="space-y-1">
            <Label>Preferred Provider</Label>
            <Select
              value={preferredProviderId || NO_PROVIDER}
              onValueChange={(value) =>
                setPreferredProviderId(value === NO_PROVIDER ? "" : value)
              }
            >
              <SelectTrigger>
                <SelectValue placeholder="No preferred provider" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={NO_PROVIDER}>No preferred provider</SelectItem>
                {providers.map((provider) => (
                  <SelectItem key={provider.provider} value={provider.provider}>
                    {provider.provider}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <Button type="submit" disabled={isSaving}>
            <Save className="size-4" />
            {isSaving ? "Saving..." : "Save Behavior"}
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => void handleDelete()}
            disabled={isDeleting}
          >
            <Trash2 className="size-4" />
            {isDeleting ? "Deleting..." : "Delete Behavior"}
          </Button>
        </div>
      </form>
    </section>
  );
}
