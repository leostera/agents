import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
  Switch,
  Textarea,
} from "@borg/ui";
import { Code2, type LucideIcon, Puzzle, TerminalSquare } from "lucide-react";
import React from "react";

export type AddCapabilityInput = {
  name: string;
  hint: string;
  mode: string;
  instructions: string;
  status: string;
};

type CapabilityModeOption = {
  id: string;
  label: string;
  icon: LucideIcon;
};

const CAPABILITY_MODE_OPTIONS: CapabilityModeOption[] = [
  { id: "codemode", label: "CodeMode", icon: Code2 },
  { id: "mcp", label: "MCP", icon: Puzzle },
  { id: "shell", label: "Shell", icon: TerminalSquare },
];

type AddCapabilityFormProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  isSaving: boolean;
  title?: string;
  description?: string;
  submitLabel?: string;
  initialValue?: AddCapabilityInput | null;
  onSubmit: (input: AddCapabilityInput) => Promise<void>;
};

const DEFAULT_INPUT: AddCapabilityInput = {
  name: "",
  hint: "",
  mode: "codemode",
  instructions: "",
  status: "active",
};

export function AddCapabilityForm({
  open,
  onOpenChange,
  isSaving,
  title = "Add Capability",
  description = "Create a new capability for this app.",
  submitLabel = "Save Capability",
  initialValue = null,
  onSubmit,
}: AddCapabilityFormProps) {
  const [form, setForm] = React.useState<AddCapabilityInput>(DEFAULT_INPUT);

  React.useEffect(() => {
    if (!open) return;
    if (initialValue) {
      setForm(initialValue);
      return;
    }
    setForm(DEFAULT_INPUT);
  }, [open, initialValue]);

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    await onSubmit({
      name: form.name.trim(),
      hint: form.hint.trim(),
      mode: form.mode.trim() || "codemode",
      instructions: form.instructions.trim(),
      status: form.status.trim() || "active",
    });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <form className="space-y-3" onSubmit={handleSubmit}>
          <div className="space-y-1">
            <Label htmlFor="capability-name">Name</Label>
            <Input
              id="capability-name"
              value={form.name}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  name: event.currentTarget.value,
                }))
              }
              placeholder="searchCalendar"
              required
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="capability-hint">Hint for LLM</Label>
            <Input
              id="capability-hint"
              value={form.hint}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  hint: event.currentTarget.value,
                }))
              }
              placeholder="Use this to find events by date range"
              required
            />
          </div>
          <div className="space-y-1">
            <Label>Mode</Label>
            <div className="grid grid-cols-3 gap-2">
              {CAPABILITY_MODE_OPTIONS.map((option) => {
                const ModeIcon = option.icon;
                const isSelected = form.mode === option.id;
                return (
                  <Button
                    key={option.id}
                    type="button"
                    variant={isSelected ? "default" : "outline"}
                    className="h-12 justify-start gap-2"
                    onClick={() =>
                      setForm((current) => ({ ...current, mode: option.id }))
                    }
                  >
                    <ModeIcon className="size-4" />
                    {option.label}
                  </Button>
                );
              })}
            </div>
          </div>
          <div className="space-y-1">
            <Label htmlFor="capability-instructions">Instructions</Label>
            <Textarea
              id="capability-instructions"
              value={form.instructions}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  instructions: event.currentTarget.value,
                }))
              }
              rows={6}
              placeholder="Describe how this capability behaves and what it expects."
              required
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="capability-status">Status</Label>
            <div className="flex items-center gap-3">
              <Switch
                id="capability-status"
                checked={form.status === "active"}
                onCheckedChange={(checked) =>
                  setForm((current) => ({
                    ...current,
                    status: checked ? "active" : "disabled",
                  }))
                }
              />
              <span className="text-sm">
                {form.status === "active" ? "Active" : "Disabled"}
              </span>
            </div>
          </div>

          <DialogFooter>
            <Button type="submit" disabled={isSaving}>
              {isSaving ? "Saving..." : submitLabel}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
