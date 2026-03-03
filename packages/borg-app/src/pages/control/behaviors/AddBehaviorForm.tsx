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

export type AddBehaviorInput = {
  behaviorId: string;
  name: string;
  systemPrompt: string;
  preferredProviderId?: string;
};

type AddBehaviorFormProps = {
  providers: { provider: string; enabled: boolean }[];
  open: boolean;
  isSaving: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (input: AddBehaviorInput) => Promise<void>;
};

type FormState = {
  name: string;
  systemPrompt: string;
  preferredProviderId: string;
};

const DEFAULT_FORM: FormState = {
  name: "",
  systemPrompt: "You are a helpful actor behavior.",
  preferredProviderId: "",
};

function createBehaviorUri(): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return `borg:behavior:${crypto.randomUUID()}`;
  }
  return `borg:behavior:${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`;
}

export function AddBehaviorForm({
  providers,
  open,
  isSaving,
  onOpenChange,
  onSubmit,
}: AddBehaviorFormProps) {
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
      behaviorId: createBehaviorUri(),
      name,
      systemPrompt,
      preferredProviderId: form.preferredProviderId.trim() || undefined,
    });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Add Behavior</DialogTitle>
        </DialogHeader>
        <form className="space-y-3" onSubmit={handleSubmit}>
          <div className="space-y-1">
            <Label htmlFor="behavior-name">Name</Label>
            <Input
              id="behavior-name"
              value={form.name}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  name: event.currentTarget.value,
                }))
              }
              placeholder="Prototyping"
              required
            />
          </div>

          <div className="space-y-1">
            <Label htmlFor="behavior-system-prompt">System Prompt</Label>
            <Textarea
              id="behavior-system-prompt"
              rows={6}
              value={form.systemPrompt}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  systemPrompt: event.currentTarget.value,
                }))
              }
              placeholder="You optimize for rapid iteration."
              required
            />
          </div>

          <div className="space-y-1">
            <Label>Preferred Provider (optional)</Label>
            <Select
              value={form.preferredProviderId || "__none__"}
              onValueChange={(value) =>
                setForm((current) => ({
                  ...current,
                  preferredProviderId: value === "__none__" ? "" : value,
                }))
              }
            >
              <SelectTrigger>
                <SelectValue placeholder="No preferred provider" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__none__">No preferred provider</SelectItem>
                {providers.map((provider) => (
                  <SelectItem key={provider.provider} value={provider.provider}>
                    {provider.provider}
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
              {isSaving ? "Creating..." : "Create Behavior"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
