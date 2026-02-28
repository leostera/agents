import { createBorgApiClient, type ProviderRecord } from "@borg/api";
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Textarea,
} from "@borg/ui";
import { LoaderCircle } from "lucide-react";
import React from "react";

const borgApi = createBorgApiClient();

export type AddAgentInput = {
  agentId: string;
  name: string;
  provider: string;
  model: string;
  systemPrompt: string;
  tools: unknown;
};

type AddAgentFormProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  isSaving: boolean;
  onSubmit: (input: AddAgentInput) => Promise<void>;
};

type FormState = {
  name: string;
  provider: string;
  model: string;
  systemPrompt: string;
  toolsJson: string;
};

const DEFAULT_FORM: FormState = {
  name: "",
  provider: "",
  model: "",
  systemPrompt: "",
  toolsJson: "[]",
};

function createAgentUri(): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return `borg:agent:${crypto.randomUUID()}`;
  }
  const fallback = `${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`;
  return `borg:agent:${fallback}`;
}

function parseToolsJson(raw: string): unknown {
  const value = raw.trim();
  if (!value) return [];
  return JSON.parse(value);
}

export function AddAgentForm({
  open,
  onOpenChange,
  isSaving,
  onSubmit,
}: AddAgentFormProps) {
  const [form, setForm] = React.useState<FormState>(DEFAULT_FORM);
  const [providers, setProviders] = React.useState<ProviderRecord[]>([]);
  const [models, setModels] = React.useState<string[]>([]);
  const [isLoadingProviders, setIsLoadingProviders] = React.useState(false);
  const [isLoadingModels, setIsLoadingModels] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const loadModelsForProvider = React.useCallback(
    async (provider: string, preferredModel?: string) => {
      if (!provider) {
        setModels([]);
        setForm((current) => ({ ...current, model: "" }));
        return;
      }
      setIsLoadingModels(true);
      try {
        const rows = await borgApi.listProviderModels(provider);
        setModels(rows);
        setForm((current) => ({
          ...current,
          model:
            preferredModel && rows.includes(preferredModel)
              ? preferredModel
              : (rows[0] ?? ""),
        }));
      } catch (loadError) {
        setModels([]);
        setForm((current) => ({ ...current, model: "" }));
        setError(
          loadError instanceof Error
            ? loadError.message
            : "Unable to load provider models"
        );
      } finally {
        setIsLoadingModels(false);
      }
    },
    []
  );

  React.useEffect(() => {
    if (!open) {
      setForm(DEFAULT_FORM);
      setProviders([]);
      setModels([]);
      setError(null);
      return;
    }

    let active = true;
    setIsLoadingProviders(true);
    setError(null);

    void borgApi
      .listProviders(100)
      .then(async (rows) => {
        if (!active) return;
        setProviders(rows);
        const defaultProvider = rows[0]?.provider ?? "";
        setForm((current) => ({
          ...current,
          provider: defaultProvider,
        }));
        if (defaultProvider) {
          await loadModelsForProvider(defaultProvider);
        }
      })
      .catch((loadError) => {
        if (!active) return;
        setProviders([]);
        setError(
          loadError instanceof Error
            ? loadError.message
            : "Unable to load providers"
        );
      })
      .finally(() => {
        if (!active) return;
        setIsLoadingProviders(false);
      });

    return () => {
      active = false;
    };
  }, [open, loadModelsForProvider]);

  const handleProviderChange = async (provider: string) => {
    setError(null);
    setForm((current) => ({
      ...current,
      provider,
      model: "",
    }));
    await loadModelsForProvider(provider);
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);

    let tools: unknown;
    try {
      tools = parseToolsJson(form.toolsJson);
    } catch {
      setError("Tools must be valid JSON");
      return;
    }

    if (!form.provider) {
      setError("Connect a provider first");
      return;
    }
    if (!form.model) {
      setError("Select a model");
      return;
    }

    await onSubmit({
      agentId: createAgentUri(),
      name: form.name.trim(),
      provider: form.provider,
      model: form.model,
      systemPrompt: form.systemPrompt,
      tools,
    });
  };

  const noProviders = !isLoadingProviders && providers.length === 0;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Add Agent</DialogTitle>
          <DialogDescription>
            Configure a new agent with provider and model defaults.
          </DialogDescription>
        </DialogHeader>

        <form className="space-y-3" onSubmit={handleSubmit}>
          <div className="space-y-1">
            <Label htmlFor="agent-name">Name</Label>
            <Input
              id="agent-name"
              value={form.name}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  name: event.currentTarget.value,
                }))
              }
              placeholder="Support Agent"
              required
            />
          </div>

          <div className="grid gap-3 md:grid-cols-2">
            <div className="space-y-1">
              <Label>Provider</Label>
              <Select
                value={form.provider}
                onValueChange={(value) => void handleProviderChange(value)}
              >
                <SelectTrigger disabled={isLoadingProviders || noProviders}>
                  <SelectValue
                    placeholder={
                      isLoadingProviders
                        ? "Loading providers..."
                        : "Select provider"
                    }
                  />
                </SelectTrigger>
                <SelectContent>
                  {providers.map((provider) => (
                    <SelectItem
                      key={provider.provider}
                      value={provider.provider}
                    >
                      {provider.provider}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-1">
              <Label>Model</Label>
              <Select
                value={form.model}
                onValueChange={(value) =>
                  setForm((current) => ({ ...current, model: value }))
                }
              >
                <SelectTrigger
                  disabled={
                    !form.provider || isLoadingModels || models.length === 0
                  }
                >
                  <SelectValue
                    placeholder={
                      isLoadingModels ? "Loading models..." : "Select model"
                    }
                  />
                </SelectTrigger>
                <SelectContent>
                  {models.map((model) => (
                    <SelectItem key={model} value={model}>
                      {model}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>

          <div className="space-y-1">
            <Label htmlFor="agent-system-prompt">System Prompt</Label>
            <Textarea
              id="agent-system-prompt"
              value={form.systemPrompt}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  systemPrompt: event.currentTarget.value,
                }))
              }
              placeholder="You are a helpful assistant..."
              rows={4}
            />
          </div>

          <div className="space-y-1">
            <Label htmlFor="agent-tools">Tools (JSON)</Label>
            <Textarea
              id="agent-tools"
              value={form.toolsJson}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  toolsJson: event.currentTarget.value,
                }))
              }
              rows={4}
            />
          </div>

          {noProviders ? (
            <p className="text-xs text-destructive">
              No providers configured. Connect one in Settings &gt; Providers.
            </p>
          ) : null}
          {error ? <p className="text-xs text-destructive">{error}</p> : null}

          <DialogFooter>
            <Button
              type="submit"
              disabled={isSaving || noProviders || !form.model}
            >
              {isSaving ? (
                <LoaderCircle className="size-4 animate-spin" />
              ) : null}
              Save Agent
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
