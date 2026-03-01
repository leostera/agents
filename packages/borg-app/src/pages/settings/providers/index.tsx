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
import { ConnectProviderForm } from "./ConnectProviderForm";

const borgApi = createBorgApiClient();

function formatProviderName(provider: string): string {
  if (provider === "openai") return "OpenAI";
  if (provider === "openrouter") return "OpenRouter";
  return provider;
}

function formatTimestamp(value?: string | null): string {
  if (!value) return "—";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.valueOf())) return "—";
  return parsed.toLocaleString();
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
                    <div className="flex flex-col">
                      <span className="font-medium">
                        {formatProviderName(provider.provider)}
                      </span>
                      <span className="text-muted-foreground text-xs">
                        {provider.provider}
                      </span>
                    </div>
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
                          aria-label={`Edit ${formatProviderName(provider.provider)}`}
                          title="Edit"
                        >
                          <Pencil className="size-3.5" />
                        </Link>
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

      {!showEmptyState && errorMessage ? (
        <p className="inline-flex items-center gap-2 text-xs text-destructive">
          <TriangleAlert className="size-3.5" />
          {errorMessage}
        </p>
      ) : null}
    </Section>
  );
}
