import {
  Button,
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Textarea,
} from "@borg/ui";
import React from "react";

export type AddActorInput = {
  actorId: string;
  name: string;
  systemPrompt: string;
  defaultBehaviorId: string;
};

type AddActorFormProps = {
  behaviors: { behavior_id: string; name: string; status: string }[];
  open: boolean;
  isSaving: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (input: AddActorInput) => Promise<void>;
};

type FormState = {
  name: string;
  systemPrompt: string;
  defaultBehaviorId: string;
};

const DEFAULT_FORM: FormState = {
  name: "",
  systemPrompt: "You are a helpful long-running actor.",
  defaultBehaviorId: "",
};

function createActorUri(): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return `borg:actor:${crypto.randomUUID()}`;
  }
  return `borg:actor:${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`;
}

const NO_BEHAVIOR = "__none__";

export function AddActorForm({
  behaviors,
  open,
  isSaving,
  onOpenChange,
  onSubmit,
}: AddActorFormProps) {
  const [form, setForm] = React.useState<FormState>(DEFAULT_FORM);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    if (!open) {
      setForm(DEFAULT_FORM);
      setError(null);
    }
  }, [open]);

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);

    const name = form.name.trim();
    const systemPrompt = form.systemPrompt.trim();
    if (!name) {
      setError("Name is required");
      return;
    }
    if (!systemPrompt) {
      setError("System prompt is required");
      return;
    }
    if (!form.defaultBehaviorId.trim()) {
      setError("Default behavior is required");
      return;
    }

    await onSubmit({
      actorId: createActorUri(),
      name,
      systemPrompt,
      defaultBehaviorId: form.defaultBehaviorId.trim(),
    });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Add Actor</DialogTitle>
        </DialogHeader>
        <form className="space-y-3" onSubmit={handleSubmit}>
          <div className="space-y-1">
            <Label htmlFor="actor-name">Name</Label>
            <Input
              id="actor-name"
              value={form.name}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  name: event.currentTarget.value,
                }))
              }
              placeholder="DevMode Integrator"
              required
            />
          </div>

          <div className="space-y-1">
            <Label htmlFor="actor-system-prompt">System Prompt</Label>
            <Textarea
              id="actor-system-prompt"
              rows={6}
              value={form.systemPrompt}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  systemPrompt: event.currentTarget.value,
                }))
              }
              placeholder="You are an actor that handles many sessions concurrently."
              required
            />
          </div>

          <div className="space-y-1">
            <Label>Default Behavior</Label>
            <Select
              value={form.defaultBehaviorId || NO_BEHAVIOR}
              onValueChange={(value) =>
                setForm((current) => ({
                  ...current,
                  defaultBehaviorId: value === NO_BEHAVIOR ? "" : value,
                }))
              }
            >
              <SelectTrigger>
                <SelectValue placeholder="Select default behavior" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={NO_BEHAVIOR} disabled>
                  Select default behavior
                </SelectItem>
                {behaviors
                  .filter((behavior) => behavior.status === "ACTIVE")
                  .map((behavior) => (
                    <SelectItem
                      key={behavior.behavior_id}
                      value={behavior.behavior_id}
                    >
                      {behavior.name} ({behavior.behavior_id})
                    </SelectItem>
                  ))}
              </SelectContent>
            </Select>
          </div>

          {error ? <p className="text-destructive text-xs">{error}</p> : null}

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isSaving}>
              {isSaving ? "Creating..." : "Create Actor"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
