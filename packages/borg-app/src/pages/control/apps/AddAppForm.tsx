import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
  Switch,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Textarea,
} from "@borg/ui";
import {
  AppWindow,
  Blocks,
  Bot,
  Code2,
  Github,
  type LucideIcon,
  Puzzle,
  TerminalSquare,
} from "lucide-react";
import React from "react";

export type AppCapabilityInput = {
  name: string;
  hint: string;
  mode: string;
  instructions: string;
};

export type AddAppInput = {
  appKind: string;
  appId: string;
  name: string;
  description: string;
  status: string;
  availableSecrets: string[];
  capability: AppCapabilityInput;
};

type AppKindOption = {
  id: string;
  label: string;
  icon: LucideIcon;
};

type CapabilityModeOption = {
  id: string;
  label: string;
  icon: LucideIcon;
};

const APP_KIND_OPTIONS: AppKindOption[] = [
  { id: "codemode", label: "CodeMode", icon: Code2 },
  { id: "github", label: "GitHub", icon: Github },
  { id: "mcp", label: "MCP", icon: Puzzle },
  { id: "workflow", label: "Workflow", icon: Blocks },
  { id: "agent", label: "Agent", icon: Bot },
];

const CAPABILITY_MODE_OPTIONS: CapabilityModeOption[] = [
  { id: "codemode", label: "CodeMode", icon: Code2 },
  { id: "mcp", label: "MCP", icon: Puzzle },
  { id: "shell", label: "Shell", icon: TerminalSquare },
];

type AddAppFormProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  isSaving: boolean;
  onSubmit: (input: AddAppInput) => Promise<void>;
  onStartGithubOAuth: () => Promise<void>;
};

const DEFAULT_SECRET_NAME = "";
const DEFAULT_CAPABILITY: AppCapabilityInput = {
  name: "",
  hint: "",
  mode: "codemode",
  instructions: "",
};

function nextAppId(): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return `borg:app:${crypto.randomUUID()}`;
  }
  return `borg:app:${Date.now()}`;
}

export function AddAppForm({
  open,
  onOpenChange,
  isSaving,
  onSubmit,
  onStartGithubOAuth,
}: AddAppFormProps) {
  const [dialogStep, setDialogStep] = React.useState<
    "kind" | "details" | "capability" | "oauth"
  >("kind");
  const [selectedAppKind, setSelectedAppKind] = React.useState("codemode");
  const [appId, setAppId] = React.useState(nextAppId);
  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [status, setStatus] = React.useState("active");
  const [availableSecrets, setAvailableSecrets] = React.useState<string[]>([]);
  const [capability, setCapability] = React.useState(DEFAULT_CAPABILITY);
  const [isStartingOAuth, setIsStartingOAuth] = React.useState(false);

  React.useEffect(() => {
    if (!open) {
      setDialogStep("kind");
      setSelectedAppKind("codemode");
      setAppId(nextAppId());
      setName("");
      setDescription("");
      setStatus("active");
      setAvailableSecrets([]);
      setCapability(DEFAULT_CAPABILITY);
      setIsStartingOAuth(false);
    }
  }, [open]);

  const canContinueFromDetails =
    appId.trim().length > 0 && name.trim().length > 0;

  const addSecretRow = () => {
    setAvailableSecrets((current) => [...current, DEFAULT_SECRET_NAME]);
  };

  const updateSecret = (index: number, value: string): void => {
    setAvailableSecrets((current) =>
      current.map((secretName, rowIndex) =>
        rowIndex === index ? value : secretName
      )
    );
  };

  const removeSecret = (index: number) => {
    setAvailableSecrets((current) =>
      current.filter((_, rowIndex) => rowIndex !== index)
    );
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();

    const normalizedSecrets = availableSecrets
      .map((name) => name.trim())
      .filter((name) => name.length > 0);

    await onSubmit({
      appKind: selectedAppKind,
      appId: appId.trim(),
      name: name.trim(),
      description: description.trim(),
      status: status.trim() || "active",
      availableSecrets: normalizedSecrets,
      capability: {
        name: capability.name.trim(),
        hint: capability.hint.trim(),
        mode: capability.mode.trim() || "codemode",
        instructions: capability.instructions.trim(),
      },
    });
  };

  const handleStartGithubOAuth = async () => {
    setIsStartingOAuth(true);
    try {
      await onStartGithubOAuth();
    } finally {
      setIsStartingOAuth(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Add App</DialogTitle>
          <DialogDescription>
            Choose app kind, configure details and secrets, then add the first
            capability.
          </DialogDescription>
        </DialogHeader>

        {dialogStep === "kind" ? (
          <div className="space-y-2">
            <Label>App kind</Label>
            <div className="grid grid-cols-2 gap-2">
              {APP_KIND_OPTIONS.map((option) => {
                const Icon = option.icon;
                const isEnabled =
                  option.id === "codemode" || option.id === "github";
                return (
                  <Button
                    key={option.id}
                    type="button"
                    variant="outline"
                    className="h-16 justify-start gap-3"
                    disabled={!isEnabled}
                    onClick={() => {
                      if (!isEnabled) return;
                      setSelectedAppKind(option.id);
                      setDialogStep(
                        option.id === "github" ? "oauth" : "details"
                      );
                    }}
                  >
                    <Icon className="size-5" />
                    <span className="flex items-center gap-2">
                      {option.label}
                      {isEnabled ? null : (
                        <span className="text-muted-foreground text-[10px] uppercase tracking-wide">
                          Coming soon
                        </span>
                      )}
                    </span>
                  </Button>
                );
              })}
            </div>
          </div>
        ) : null}

        {dialogStep === "oauth" ? (
          <div className="space-y-3">
            <div className="rounded-md border p-4">
              <div className="mb-2 flex items-center gap-2 text-sm font-medium">
                <Github className="size-4" />
                GitHub App Connection
              </div>
              <p className="text-muted-foreground text-sm">
                Connect this Borg instance to GitHub using Borg's managed GitHub
                OAuth app.
              </p>
            </div>
            <div className="flex items-center justify-end gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() => setDialogStep("kind")}
                disabled={isStartingOAuth}
              >
                Back
              </Button>
              <Button
                type="button"
                onClick={() => void handleStartGithubOAuth()}
                disabled={isStartingOAuth}
              >
                {isStartingOAuth ? "Starting..." : "Sign in with GitHub"}
              </Button>
            </div>
          </div>
        ) : null}

        {dialogStep === "details" ? (
          <div className="space-y-3">
            <div className="space-y-1">
              <Label htmlFor="app-id">App ID (URI)</Label>
              <Input id="app-id" value={appId} readOnly disabled />
            </div>
            <div className="grid grid-cols-1 gap-3">
              <div className="space-y-1">
                <Label htmlFor="app-name">Name</Label>
                <Input
                  id="app-name"
                  value={name}
                  onChange={(event) => setName(event.currentTarget.value)}
                  placeholder="MovieIndex"
                />
              </div>
            </div>
            <div className="space-y-1">
              <Label htmlFor="app-description">Description</Label>
              <Textarea
                id="app-description"
                value={description}
                onChange={(event) => setDescription(event.currentTarget.value)}
                rows={3}
                placeholder="Short app description and purpose"
              />
            </div>
            <div className="space-y-1">
              <Label htmlFor="app-status">Status</Label>
              <div className="flex items-center gap-3">
                <Switch
                  id="app-status"
                  checked={status === "active"}
                  onCheckedChange={(checked) =>
                    setStatus(checked ? "active" : "disabled")
                  }
                />
                <span className="text-sm">
                  {status === "active" ? "Active" : "Disabled"}
                </span>
              </div>
            </div>

            <section className="space-y-2">
              <div className="flex items-center justify-between">
                <Label>Available Secrets</Label>
                <Button type="button" variant="outline" onClick={addSecretRow}>
                  Add Secret
                </Button>
              </div>
              {availableSecrets.length === 0 ? (
                <p className="text-muted-foreground text-xs">
                  No secret names configured yet.
                </p>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Secret name</TableHead>
                      <TableHead className="w-[84px]">Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {availableSecrets.map((secretName, index) => (
                      <TableRow key={`${secretName}-${index}`}>
                        <TableCell>
                          <Input
                            value={secretName}
                            onChange={(event) =>
                              updateSecret(index, event.currentTarget.value)
                            }
                            placeholder="APP_GITHUB_ACCESS_TOKEN"
                          />
                        </TableCell>
                        <TableCell>
                          <Button
                            type="button"
                            variant="outline"
                            onClick={() => removeSecret(index)}
                          >
                            Remove
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </section>

            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() => setDialogStep("kind")}
              >
                Back
              </Button>
              <Button
                type="button"
                onClick={() => setDialogStep("capability")}
                disabled={!canContinueFromDetails}
              >
                Continue
              </Button>
            </div>
          </div>
        ) : null}

        {dialogStep === "capability" ? (
          <form className="space-y-3" onSubmit={handleSubmit}>
            <div className="rounded-md border p-3">
              <div className="flex items-center gap-2 text-sm">
                <AppWindow className="size-4" />
                <span>
                  Creating capability for <strong>{name || appId}</strong>
                </span>
              </div>
            </div>
            <div className="space-y-1">
              <Label htmlFor="capability-name">Capability name</Label>
              <Input
                id="capability-name"
                value={capability.name}
                onChange={(event) =>
                  setCapability((current) => ({
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
                value={capability.hint}
                onChange={(event) =>
                  setCapability((current) => ({
                    ...current,
                    hint: event.currentTarget.value,
                  }))
                }
                placeholder="Use this to find calendar events by date range"
                required
              />
            </div>
            <div className="space-y-1">
              <Label>Mode</Label>
              <div className="grid grid-cols-3 gap-2">
                {CAPABILITY_MODE_OPTIONS.map((modeOption) => {
                  const ModeIcon = modeOption.icon;
                  const isSelected = capability.mode === modeOption.id;
                  return (
                    <Button
                      key={modeOption.id}
                      type="button"
                      variant={isSelected ? "default" : "outline"}
                      className="h-12 justify-start gap-2"
                      onClick={() =>
                        setCapability((current) => ({
                          ...current,
                          mode: modeOption.id,
                        }))
                      }
                    >
                      <ModeIcon className="size-4" />
                      {modeOption.label}
                    </Button>
                  );
                })}
              </div>
            </div>
            <div className="space-y-1">
              <Label htmlFor="capability-instructions">Instructions</Label>
              <Textarea
                id="capability-instructions"
                value={capability.instructions}
                onChange={(event) =>
                  setCapability((current) => ({
                    ...current,
                    instructions: event.currentTarget.value,
                  }))
                }
                rows={6}
                placeholder="Describe how this capability should behave and what inputs it expects."
                required
              />
            </div>
            <div className="flex items-center justify-end gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() => setDialogStep("details")}
              >
                Back
              </Button>
              <Button type="submit" disabled={isSaving}>
                {isSaving ? "Saving..." : "Save App"}
              </Button>
            </div>
          </form>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
