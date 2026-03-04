import {
  BorgApiError,
  createBorgApiClient,
  type ProviderRecord,
} from "@borg/api";
import {
  Badge,
  Button,
  Link,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@borg/ui";
import {
  CheckCircle2,
  LoaderCircle,
  Pause,
  Pencil,
  Play,
  Settings2,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import React from "react";
import {
  Section,
  SectionContent,
  SectionEmpty,
  SectionToolbar,
} from "../../../components/Section";
import {
  ConnectProviderForm,
  type ConnectProviderInput,
} from "./ConnectProviderForm";

const borgApi = createBorgApiClient();

function formatProviderKind(kind: string): string {
  if (kind === "openai") return "OpenAI";
  if (kind === "openrouter") return "OpenRouter";
  if (kind === "lmstudio") return "LM Studio";
  if (kind === "ollama") return "Ollama";
  return kind;
}

function isLocalProvider(providerKind: string): boolean {
  return providerKind === "lmstudio" || providerKind === "ollama";
}

function formatTimestamp(value?: string | null): string {
  if (!value) return "—";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.valueOf())) return "—";
  return parsed.toLocaleString();
}

function formatProviderName(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return "—";

  if (trimmed.startsWith("borg:provider:")) {
    return trimmed.slice("borg:provider:".length) || trimmed;
  }

  if (trimmed.includes("://")) {
    try {
      const parsed = new URL(trimmed);
      const pathParts = parsed.pathname.split("/").filter(Boolean);
      const lastPathPart = pathParts[pathParts.length - 1];
      if (lastPathPart) return lastPathPart;
      return parsed.hostname || trimmed;
    } catch {
      return trimmed;
    }
  }

  return trimmed;
}

export function ProvidersPage() {
  const [providersByName, setProvidersByName] = React.useState<
    Record<string, ProviderRecord>
  >({});
  const [isLoading, setIsLoading] = React.useState(true);
  const [isDialogOpen, setIsDialogOpen] = React.useState(false);
  const [isSavingConnect, setIsSavingConnect] = React.useState(false);
  const [isStartingOpenAi, setIsStartingOpenAi] = React.useState(false);
  const [statusMessage, setStatusMessage] = React.useState<string | null>(null);
  const [errorMessage, setErrorMessage] = React.useState<string | null>(null);

  const loadProviders = React.useCallback(async () => {
    setIsLoading(true);
    setErrorMessage(null);
    try {
      const providers = await borgApi.listProviders(100);
      const byName = Object.fromEntries(
        providers.map((provider) => [provider.provider, provider])
      );
      setProvidersByName(byName);
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

  const handleConnectProvider = async (input: ConnectProviderInput) => {
    const apiKey = input.apiKey?.trim();
    const baseUrl = input.baseUrl?.trim();
    if (!input.providerId.trim()) {
      setErrorMessage("Provider ID is required");
      return;
    }
    if (!isLocalProvider(input.providerKind) && !apiKey) {
      setErrorMessage(
        `${formatProviderKind(input.providerKind)} API key is required`
      );
      return;
    }
    if (isLocalProvider(input.providerKind) && !baseUrl) {
      setErrorMessage(
        `${formatProviderKind(input.providerKind)} base URL is required`
      );
      return;
    }

    setIsSavingConnect(true);
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      await borgApi.upsertProvider({
        provider: input.providerId,
        providerKind: input.providerKind,
        apiKey,
        baseUrl,
        enabled: true,
      });
      setStatusMessage(
        `${formatProviderKind(input.providerKind)} provider saved`
      );
      await loadProviders();
      setIsDialogOpen(false);
    } catch (error) {
      setErrorMessage(
        error instanceof Error ? error.message : "Unable to save provider"
      );
    } finally {
      setIsSavingConnect(false);
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
      const providerKind = providersByName[provider]?.provider_kind ?? provider;
      setStatusMessage(`${formatProviderKind(providerKind)} deleted`);
      await loadProviders();
    } catch (error) {
      setErrorMessage(
        error instanceof Error
          ? error.message
          : `Unable to delete provider ${provider}`
      );
    }
  };

  const handleToggleEnabled = async (provider: ProviderRecord) => {
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      await borgApi.upsertProvider({
        provider: provider.provider,
        providerKind: provider.provider_kind,
        apiKey: provider.api_key || undefined,
        baseUrl: provider.base_url ?? undefined,
        enabled: !provider.enabled,
      });
      setStatusMessage(
        `${formatProviderKind(provider.provider_kind)} ${provider.enabled ? "paused" : "resumed"}`
      );
      await loadProviders();
    } catch (error) {
      setErrorMessage(
        error instanceof Error
          ? error.message
          : `Unable to update ${formatProviderKind(provider.provider_kind)}`
      );
    }
  };

  const providerRows = React.useMemo(
    () =>
      Object.values(providersByName).sort((left, right) =>
        left.provider.localeCompare(right.provider)
      ),
    [providersByName]
  );
  const showEmptyState = !isLoading && providerRows.length === 0;

  return (
    <Section className="gap-4">
      {!showEmptyState ? (
        <SectionToolbar className="justify-end">
          <Button variant="outline" onClick={() => setIsDialogOpen(true)}>
            + Connect Provider
          </Button>
        </SectionToolbar>
      ) : null}

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

      <SectionContent>
        {showEmptyState ? (
          <SectionEmpty
            icon={Settings2}
            title="No Providers Configured"
            description="No providers configured yet. Connect your first provider."
            action={
              <Button onClick={() => setIsDialogOpen(true)}>
                + Connect Provider
              </Button>
            }
          />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-[44px]">Status</TableHead>
                <TableHead>Provider</TableHead>
                <TableHead>Name</TableHead>
                <TableHead>Chat Model</TableHead>
                <TableHead>Audio Model</TableHead>
                <TableHead>Tokens Used</TableHead>
                <TableHead>Last Used</TableHead>
                <TableHead>Updated</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {providerRows.map((provider) => (
                <TableRow key={provider.provider}>
                  <TableCell>
                    <div className="inline-flex items-center gap-2">
                      <span
                        className={
                          provider.enabled
                            ? "inline-block size-2.5 rounded-full bg-emerald-500"
                            : "inline-block size-2.5 rounded-full bg-rose-500"
                        }
                        title={provider.enabled ? "Enabled" : "Disabled"}
                      />
                    </div>
                  </TableCell>
                  <TableCell>
                    {formatProviderKind(provider.provider_kind)}
                  </TableCell>
                  <TableCell>
                    <span className="font-medium">
                      {formatProviderName(provider.provider)}
                    </span>
                  </TableCell>
                  <TableCell>
                    {provider.default_text_model ? (
                      <Badge variant="outline">
                        {provider.default_text_model}
                      </Badge>
                    ) : (
                      "—"
                    )}
                  </TableCell>
                  <TableCell>
                    {provider.default_audio_model ? (
                      <Badge variant="outline">
                        {provider.default_audio_model}
                      </Badge>
                    ) : (
                      "—"
                    )}
                  </TableCell>
                  <TableCell>{provider.tokens_used.toLocaleString()}</TableCell>
                  <TableCell>{formatTimestamp(provider.last_used)}</TableCell>
                  <TableCell>{formatTimestamp(provider.updated_at)}</TableCell>
                  <TableCell>
                    <div className="flex items-center gap-1">
                      <Button size="icon-sm" variant="outline" asChild>
                        <Link
                          href={`/settings/providers/${provider.provider}`}
                          aria-label={`Edit ${formatProviderKind(provider.provider_kind)} ${provider.provider}`}
                          title="Edit"
                        >
                          <Pencil className="size-3.5" />
                        </Link>
                      </Button>
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={() => void handleToggleEnabled(provider)}
                        aria-label={`${provider.enabled ? "Pause" : "Resume"} ${formatProviderKind(provider.provider_kind)} ${provider.provider}`}
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
                        aria-label={`Delete ${formatProviderKind(provider.provider_kind)} ${provider.provider}`}
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
      </SectionContent>

      {showEmptyState && errorMessage ? (
        <p className="inline-flex items-center gap-2 text-xs text-destructive">
          <TriangleAlert className="size-3.5" />
          {errorMessage}
        </p>
      ) : null}

      <ConnectProviderForm
        open={isDialogOpen}
        onOpenChange={setIsDialogOpen}
        isStartingOpenAi={isStartingOpenAi}
        isSaving={isSavingConnect}
        onStartOpenAiSignIn={() => void handleStartOpenAiSignIn()}
        onSave={(input) => void handleConnectProvider(input)}
      />

      {!showEmptyState && errorMessage ? (
        <p className="inline-flex items-center gap-2 text-xs text-destructive">
          <TriangleAlert className="size-3.5" />
          {errorMessage}
        </p>
      ) : null}
    </Section>
  );
}
