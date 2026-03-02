import { Button, Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, Input, Label, Textarea } from "@borg/ui";
import React from "react";

export type AddActorInput = {
  actorId: string;
  name: string;
  systemPrompt: string;
};

type AddActorFormProps = {
  open: boolean;
  isSaving: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (input: AddActorInput) => Promise<void>;
};

type FormState = {
  name: string;
  systemPrompt: string;
};

const DEFAULT_FORM: FormState = {
  name: "",
  systemPrompt: "You are a helpful long-running actor.",
};

function createActorUri(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return `borg:actor:${crypto.randomUUID()}`;
  }
  return `borg:actor:${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`;
}

export function AddActorForm({ open, isSaving, onOpenChange, onSubmit }: AddActorFormProps) {
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

    await onSubmit({
      actorId: createActorUri(),
      name,
      systemPrompt,
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
                setForm((current) => ({ ...current, name: event.currentTarget.value }))
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

          {error ? <p className="text-destructive text-xs">{error}</p> : null}

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
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
