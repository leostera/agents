import {
  BorgApiError,
  createBorgApiClient,
  type ProviderModelsResponse,
  type ProviderRecord,
} from "@borg/api";
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@borg/ui";
import {
  CheckCircle2,
  Cpu,
  LoaderCircle,
  Pause,
  Pencil,
  Play,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import React from "react";
import { ConnectProviderForm } from "./ConnectProviderForm";

const borgApi = createBorgApiClient();
const RUNTIME_PORT_URI = "borg:port:runtime";

function formatProviderName(provider: string): string {
  if (provider === "openai") return "OpenAI";
  if (provider === "openrouter") return "OpenRouter";
  return provider;
}

type EditProviderState = {
  provider: string;
  apiKey: string;
  chatModel: string | null;
  audioModel: string | null;
};

type ProviderDefaults = {
  chatModel: string | null;
  audioModel: string | null;
};

function chatModelSettingKey(provider: string): string {
  return `provider:${provider}:default_chat_model`;
}

function audioModelSettingKey(provider: string): string {
  return `provider:${provider}:default_audio_model`;
}

export function ProvidersPage() {
  const [providersByName, setProvidersByName] = React.useState<
    Record<string, ProviderRecord>
  >({});
  const [isLoading, setIsLoading] = React.useState(true);
  const [isDialogOpen, setIsDialogOpen] = React.useState(false);
  const [openAiApiKey, setOpenAiApiKey] = React.useState("");
  const [openRouterApiKey, setOpenRouterApiKey] = React.useState("");
  const [isSavingOpenAi, setIsSavingOpenAi] = React.useState(false);
  const [isSavingOpenRouter, setIsSavingOpenRouter] = React.useState(false);
  const [isStartingOpenAi, setIsStartingOpenAi] = React.useState(false);
  const [statusMessage, setStatusMessage] = React.useState<string | null>(null);
  const [errorMessage, setErrorMessage] = React.useState<string | null>(null);
  const [editingProvider, setEditingProvider] =
    React.useState<EditProviderState | null>(null);
  const [isSavingEdit, setIsSavingEdit] = React.useState(false);
  const [providerModelsByName, setProviderModelsByName] = React.useState<
    Record<string, ProviderModelsResponse>
  >({});
  const [providerDefaultsByName, setProviderDefaultsByName] = React.useState<
    Record<string, ProviderDefaults>
  >({});

  const loadProviders = React.useCallback(async () => {
    setIsLoading(true);
    setErrorMessage(null);
    try {
      const providers = await borgApi.listProviders(100);
      const byName = Object.fromEntries(
        providers.map((provider) => [provider.provider, provider])
      );
      setProvidersByName(byName);

      const configEntries = await Promise.all(
        providers.map(async (provider) => {
          const providerName = provider.provider;
          const [modelsResult, chatModel, audioModel] = await Promise.all([
            borgApi.getProviderModels(providerName).catch(() => null),
            borgApi
              .getPortSetting(RUNTIME_PORT_URI, chatModelSettingKey(providerName))
              .catch(() => null),
            borgApi
              .getPortSetting(RUNTIME_PORT_URI, audioModelSettingKey(providerName))
              .catch(() => null),
          ]);
          return {
            provider: providerName,
            models: modelsResult ?? {},
            defaults: {
              chatModel,
              audioModel,
            },
          };
        })
      );

      setProviderModelsByName(
        Object.fromEntries(
          configEntries.map((entry) => [entry.provider, entry.models])
        )
      );
      setProviderDefaultsByName(
        Object.fromEntries(
          configEntries.map((entry) => [entry.provider, entry.defaults])
        )
      );
    } catch (error) {
      setErrorMessage(
        error instanceof Error ? error.message : "Unable to load providers"
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void loadProviders();
  }, [loadProviders]);

  React.useEffect(() => {
    const handleOpenConnect = () => setIsDialogOpen(true);
    window.addEventListener("providers:open-connect", handleOpenConnect);
    return () =>
      window.removeEventListener("providers:open-connect", handleOpenConnect);
  }, []);

  const handleSaveOpenRouter = async (
    event: React.FormEvent<HTMLFormElement>
  ) => {
    event.preventDefault();
    const apiKey = openRouterApiKey.trim();
    if (apiKey.length === 0) {
      setErrorMessage("OpenRouter API key is required");
      return;
    }

    setIsSavingOpenRouter(true);
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      await borgApi.upsertProviderApiKey("openrouter", apiKey);
      setOpenRouterApiKey("");
      setStatusMessage("OpenRouter API key saved");
      await loadProviders();
      setIsDialogOpen(false);
    } catch (error) {
      setErrorMessage(
        error instanceof Error ? error.message : "Unable to save OpenRouter key"
      );
    } finally {
      setIsSavingOpenRouter(false);
    }
  };

  const handleSaveOpenAi = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const apiKey = openAiApiKey.trim();
    if (apiKey.length === 0) {
      setErrorMessage("OpenAI API key is required");
      return;
    }

    setIsSavingOpenAi(true);
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      await borgApi.upsertProviderApiKey("openai", apiKey);
      setOpenAiApiKey("");
      setStatusMessage("OpenAI API key saved");
      await loadProviders();
      setIsDialogOpen(false);
    } catch (error) {
      setErrorMessage(
        error instanceof Error ? error.message : "Unable to save OpenAI key"
      );
    } finally {
      setIsSavingOpenAi(false);
    }
  };

  const handleStartOpenAiSignIn = async () => {
    setIsStartingOpenAi(true);
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      await borgApi.startOpenAiDeviceCode();
      setStatusMessage(
        "OpenAI device-code sign-in started. Continue in your Codex auth flow."
      );
      await loadProviders();
      setIsDialogOpen(false);
    } catch (error) {
      if (error instanceof BorgApiError && error.status === 404) {
        setErrorMessage("OpenAI device-code flow is not wired in the API yet");
      } else {
        setErrorMessage(
          error instanceof Error
            ? error.message
            : "Unable to start OpenAI sign-in"
        );
      }
    } finally {
      setIsStartingOpenAi(false);
    }
  };

  const handleDeleteProvider = async (provider: string) => {
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      await borgApi.deleteProvider(provider, { ignoreNotFound: true });
      setStatusMessage(`${formatProviderName(provider)} deleted`);
      await loadProviders();
    } catch (error) {
      setErrorMessage(
        error instanceof Error
          ? error.message
          : `Unable to delete ${formatProviderName(provider)}`
      );
    }
  };

  const handleToggleEnabled = async (provider: ProviderRecord) => {
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      await borgApi.upsertProvider({
        provider: provider.provider,
        apiKey: provider.api_key,
        enabled: !provider.enabled,
      });
      setStatusMessage(
        `${formatProviderName(provider.provider)} ${provider.enabled ? "paused" : "resumed"}`
      );
      await loadProviders();
    } catch (error) {
      setErrorMessage(
        error instanceof Error
          ? error.message
          : `Unable to update ${formatProviderName(provider.provider)}`
      );
    }
  };

  const handleSaveEditProvider = async (
    event: React.FormEvent<HTMLFormElement>
  ) => {
    event.preventDefault();
    if (!editingProvider) return;
    const apiKey = editingProvider.apiKey.trim();
    if (!apiKey) {
      setErrorMessage("API key is required");
      return;
    }

    const current = providersByName[editingProvider.provider];
    if (!current) {
      setErrorMessage("Provider no longer exists");
      return;
    }

    setIsSavingEdit(true);
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      await borgApi.upsertProvider({
        provider: editingProvider.provider,
        apiKey,
        enabled: current.enabled,
      });
      if (editingProvider.chatModel && editingProvider.chatModel.length > 0) {
        await borgApi.upsertPortSetting(
          RUNTIME_PORT_URI,
          chatModelSettingKey(editingProvider.provider),
          editingProvider.chatModel
        );
      }
      if (editingProvider.audioModel && editingProvider.audioModel.length > 0) {
        await borgApi.upsertPortSetting(
          RUNTIME_PORT_URI,
          audioModelSettingKey(editingProvider.provider),
          editingProvider.audioModel
        );
      }
      setEditingProvider(null);
      setStatusMessage(
        `${formatProviderName(editingProvider.provider)} updated`
      );
      await loadProviders();
    } catch (error) {
      setErrorMessage(
        error instanceof Error ? error.message : "Unable to update provider"
      );
    } finally {
      setIsSavingEdit(false);
    }
  };

  const providerRows = React.useMemo(
    () => Object.values(providersByName),
    [providersByName]
  );
  const showEmptyState = !isLoading && providerRows.length === 0;

  return (
    <section className="space-y-4">
      <section className="flex items-center justify-end">
        <Button variant="outline" onClick={() => setIsDialogOpen(true)}>
          + Connect Provider
        </Button>
      </section>

      {statusMessage ? (
        <div className="flex items-center gap-2 rounded-md border border-emerald-600/30 bg-emerald-600/10 px-3 py-2 text-xs text-emerald-700">
          <CheckCircle2 className="size-3.5" />
          {statusMessage}
        </div>
      ) : null}

      {isLoading ? (
        <div className="text-muted-foreground inline-flex items-center gap-2 text-xs">
          <LoaderCircle className="size-3.5 animate-spin" />
          Loading providers...
        </div>
      ) : null}

      {showEmptyState ? (
        <Empty className="border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Cpu />
            </EmptyMedia>
            <EmptyTitle>No Providers Configured</EmptyTitle>
            <EmptyDescription>
              No providers configured yet. Connect your first provider.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent className="flex-row justify-center">
            <Button onClick={() => setIsDialogOpen(true)}>
              + Connect Provider
            </Button>
          </EmptyContent>
          {errorMessage ? (
            <p className="inline-flex items-center gap-2 text-xs text-destructive">
              <TriangleAlert className="size-3.5" />
              {errorMessage}
            </p>
          ) : null}
        </Empty>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Provider</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Tokens Used</TableHead>
              <TableHead>Last Used</TableHead>
              <TableHead>Updated</TableHead>
              <TableHead>Chat Model</TableHead>
              <TableHead>Audio Model</TableHead>
              <TableHead>Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {providerRows.map((provider) => (
              <TableRow key={provider.provider}>
                <TableCell className="font-medium">
                  {formatProviderName(provider.provider)}
                </TableCell>
                <TableCell>
                  <Badge variant={provider.enabled ? "secondary" : "outline"}>
                    {provider.enabled ? "active" : "paused"}
                  </Badge>
                </TableCell>
                <TableCell>{provider.tokens_used.toLocaleString()}</TableCell>
                <TableCell>
                  {provider.last_used
                    ? new Date(provider.last_used).toLocaleString()
                    : "—"}
                </TableCell>
                <TableCell>
                  {new Date(provider.updated_at).toLocaleString()}
                </TableCell>
                <TableCell>
                  {providerDefaultsByName[provider.provider]?.chatModel ??
                  providerModelsByName[provider.provider]?.default_text_model ? (
                    <Badge variant="outline">
                      {providerDefaultsByName[provider.provider]?.chatModel ??
                        providerModelsByName[provider.provider]
                          ?.default_text_model}
                    </Badge>
                  ) : (
                    "—"
                  )}
                </TableCell>
                <TableCell>
                  {providerDefaultsByName[provider.provider]?.audioModel ??
                  providerModelsByName[provider.provider]
                    ?.default_audio_model ? (
                    <Badge variant="outline">
                      {providerDefaultsByName[provider.provider]?.audioModel ??
                        providerModelsByName[provider.provider]
                          ?.default_audio_model}
                    </Badge>
                  ) : (
                    "—"
                  )}
                </TableCell>
                <TableCell>
                  <div className="flex items-center gap-1">
                    <Button
                      size="icon-sm"
                      variant="outline"
                      onClick={() =>
                        setEditingProvider({
                          provider: provider.provider,
                          apiKey: provider.api_key,
                          chatModel:
                            providerDefaultsByName[provider.provider]
                              ?.chatModel ??
                            providerModelsByName[provider.provider]
                              ?.default_text_model ??
                            null,
                          audioModel:
                            providerDefaultsByName[provider.provider]
                              ?.audioModel ??
                            providerModelsByName[provider.provider]
                              ?.default_audio_model ??
                            null,
                        })
                      }
                      aria-label={`Edit ${formatProviderName(provider.provider)}`}
                      title="Edit"
                    >
                      <Pencil className="size-3.5" />
                    </Button>
                    <Button
                      size="icon-sm"
                      variant="outline"
                      onClick={() => void handleToggleEnabled(provider)}
                      aria-label={`${provider.enabled ? "Pause" : "Resume"} ${formatProviderName(provider.provider)}`}
                      title={provider.enabled ? "Pause" : "Resume"}
                    >
                      {provider.enabled ? (
                        <Pause className="size-3.5" />
                      ) : (
                        <Play className="size-3.5" />
                      )}
                    </Button>
                    <Button
                      size="icon-sm"
                      variant="outline"
                      onClick={() =>
                        void handleDeleteProvider(provider.provider)
                      }
                      aria-label={`Delete ${formatProviderName(provider.provider)}`}
                      title="Delete"
                    >
                      <Trash2 className="size-3.5" />
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}

      <ConnectProviderForm
        open={isDialogOpen}
        onOpenChange={setIsDialogOpen}
        isStartingOpenAi={isStartingOpenAi}
        isSavingOpenAi={isSavingOpenAi}
        isSavingOpenRouter={isSavingOpenRouter}
        openAiApiKey={openAiApiKey}
        openRouterApiKey={openRouterApiKey}
        onOpenAiApiKeyChange={setOpenAiApiKey}
        onOpenRouterApiKeyChange={setOpenRouterApiKey}
        onStartOpenAiSignIn={() => void handleStartOpenAiSignIn()}
        onSaveOpenAi={handleSaveOpenAi}
        onSaveOpenRouter={handleSaveOpenRouter}
      />

      <Dialog
        open={editingProvider !== null}
        onOpenChange={(open) => {
          if (!open) setEditingProvider(null);
        }}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Edit Provider</DialogTitle>
            <DialogDescription>
              Update API key for{" "}
              {editingProvider
                ? formatProviderName(editingProvider.provider)
                : "provider"}
              .
            </DialogDescription>
          </DialogHeader>
          <form className="space-y-3" onSubmit={handleSaveEditProvider}>
            <div className="space-y-1">
              <Label htmlFor="provider-api-key">API Key</Label>
              <Input
                id="provider-api-key"
                type="password"
                autoComplete="off"
                value={editingProvider?.apiKey ?? ""}
                onChange={(event) =>
                  setEditingProvider((current) =>
                    current
                      ? { ...current, apiKey: event.currentTarget.value }
                      : current
                  )
                }
                placeholder="sk-..."
              />
            </div>
            <div className="space-y-1">
              <Label>Default Chat Model</Label>
              <Select
                value={editingProvider?.chatModel ?? undefined}
                onValueChange={(value) =>
                  setEditingProvider((current) =>
                    current ? { ...current, chatModel: value } : current
                  )
                }
              >
                <SelectTrigger className="w-full">
                  <SelectValue placeholder="Select chat model" />
                </SelectTrigger>
                <SelectContent>
                  {(providerModelsByName[editingProvider?.provider ?? ""]?.models ?? []).map(
                    (item) => (
                      <SelectItem key={item} value={item}>
                        {item}
                      </SelectItem>
                    )
                  )}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label>Default Audio Model</Label>
              <Select
                value={editingProvider?.audioModel ?? undefined}
                onValueChange={(value) =>
                  setEditingProvider((current) =>
                    current ? { ...current, audioModel: value } : current
                  )
                }
              >
                <SelectTrigger className="w-full">
                  <SelectValue placeholder="Select audio model" />
                </SelectTrigger>
                <SelectContent>
                  {(providerModelsByName[editingProvider?.provider ?? ""]?.models ?? []).map(
                    (item) => (
                      <SelectItem key={item} value={item}>
                        {item}
                      </SelectItem>
                    )
                  )}
                </SelectContent>
              </Select>
            </div>
            <DialogFooter>
              <Button type="submit" disabled={isSavingEdit}>
                {isSavingEdit ? (
                  <LoaderCircle className="size-4 animate-spin" />
                ) : null}
                Save Provider
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </section>
  );
}
