import {
  createBorgApiClient,
  type ProviderModelsResponse,
  type ProviderRecord,
} from "@borg/api";
import {
  Badge,
  Button,
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
  Input,
  Label,
} from "@borg/ui";
import { ArrowLeft, LoaderCircle, Save, Settings2 } from "lucide-react";
import React from "react";
import {
  Section,
  SectionContent,
  SectionEmpty,
  SectionToolbar,
} from "../../../../components/Section";

const borgApi = createBorgApiClient();

type EditState = {
  provider: string;
  providerKind: string;
  apiKey: string;
  baseUrl: string;
  enabled: boolean;
  chatModel: string | null;
  audioModel: string | null;
};

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

export function ProviderDetailsPage({ providerId }: { providerId: string }) {
  const [isLoading, setIsLoading] = React.useState(true);
  const [isSaving, setIsSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [status, setStatus] = React.useState<string | null>(null);
  const [provider, setProvider] = React.useState<ProviderRecord | null>(null);
  const [models, setModels] = React.useState<ProviderModelsResponse | null>(
    null
  );
  const [form, setForm] = React.useState<EditState | null>(null);

  const load = React.useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const providers = await borgApi.listProviders(100);
      const found =
        providers.find((item) => item.provider === providerId) ?? null;
      setProvider(found);
      if (!found) {
        setForm(null);
        setModels({ provider: providerId, models: [] });
        return;
      }

      setForm({
        provider: found.provider,
        providerKind: found.provider_kind,
        apiKey: found.api_key,
        baseUrl: found.base_url ?? "",
        enabled: found.enabled,
        chatModel: found.default_text_model ?? null,
        audioModel: found.default_audio_model ?? null,
      });

      try {
        const providerModels = await borgApi.getProviderModels(found.provider);
        setModels(providerModels);
      } catch {
        setModels({ provider: found.provider, models: [] });
      }
    } catch (loadError) {
      setError(
        loadError instanceof Error
          ? loadError.message
          : "Unable to load provider"
      );
      setProvider(null);
      setForm(null);
      setModels(null);
    } finally {
      setIsLoading(false);
    }
  }, [providerId]);

  React.useEffect(() => {
    void load();
  }, [load]);

  const goBack = React.useCallback(() => {
    window.history.pushState(null, "", "/settings/providers");
    window.dispatchEvent(new PopStateEvent("popstate"));
  }, []);

  const handleSave = async () => {
    if (!form) return;
    const apiKey = form.apiKey.trim();
    const baseUrl = form.baseUrl.trim();
    if (!isLocalProvider(form.providerKind) && !apiKey) {
      setError("API key is required");
      return;
    }
    if (isLocalProvider(form.providerKind) && !baseUrl) {
      setError("Base URL is required");
      return;
    }

    setIsSaving(true);
    setError(null);
    setStatus(null);
    try {
      await borgApi.upsertProvider({
        provider: form.provider,
        providerKind: form.providerKind,
        apiKey: apiKey || undefined,
        baseUrl: baseUrl || undefined,
        enabled: form.enabled,
        defaultTextModel: form.chatModel,
        defaultAudioModel: form.audioModel,
      });
      setStatus(`${formatProviderKind(form.providerKind)} updated`);
      await load();
    } catch (saveError) {
      setError(
        saveError instanceof Error
          ? saveError.message
          : "Unable to update provider"
      );
    } finally {
      setIsSaving(false);
    }
  };

  const modelOptions = React.useMemo(() => {
    const fromApi = models?.models ?? [];
    const fromSaved = [form?.chatModel, form?.audioModel].filter(
      (value): value is string => typeof value === "string" && value.length > 0
    );
    return Array.from(new Set([...fromApi, ...fromSaved]));
  }, [form?.audioModel, form?.chatModel, models?.models]);

  return (
    <Section className="gap-4">
      <SectionToolbar className="justify-between">
        <Button variant="ghost" onClick={goBack}>
          <ArrowLeft className="size-4" />
          Back
        </Button>
        {provider ? (
          <Badge variant={provider.enabled ? "secondary" : "outline"}>
            {provider.enabled ? "enabled" : "disabled"}
          </Badge>
        ) : null}
      </SectionToolbar>

      {status ? <p className="text-xs text-emerald-700">{status}</p> : null}
      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <SectionContent>
        {isLoading ? (
          <div className="text-muted-foreground inline-flex items-center gap-2 text-xs">
            <LoaderCircle className="size-3.5 animate-spin" />
            Loading provider...
          </div>
        ) : !provider || !form ? (
          <SectionEmpty
            icon={Settings2}
            title="Provider Not Found"
            description="This provider is not configured."
            action={<Button onClick={goBack}>Back to Providers</Button>}
          />
        ) : (
          <section className="max-w-2xl space-y-4">
            <div className="space-y-1">
              <Label>Provider</Label>
              <Input value={provider.provider} disabled />
            </div>

            <div className="space-y-1">
              <Label>Provider Kind</Label>
              <Input value={formatProviderKind(provider.provider_kind)} disabled />
            </div>

            {!isLocalProvider(form.providerKind) ? (
              <div className="space-y-1">
                <Label htmlFor="provider-api-key">API Key</Label>
                <Input
                  id="provider-api-key"
                  type="password"
                  autoComplete="off"
                  value={form.apiKey}
                  onChange={(event) =>
                    setForm((current) =>
                      current
                        ? { ...current, apiKey: event.currentTarget.value }
                        : current
                    )
                  }
                  placeholder="sk-..."
                />
              </div>
            ) : (
              <div className="space-y-1">
                <Label htmlFor="provider-base-url">Base URL</Label>
                <Input
                  id="provider-base-url"
                  autoComplete="off"
                  value={form.baseUrl}
                  onChange={(event) =>
                    setForm((current) =>
                      current
                        ? { ...current, baseUrl: event.currentTarget.value }
                        : current
                    )
                  }
                  placeholder="http://127.0.0.1:1234"
                />
              </div>
            )}

            <div className="space-y-1">
              <Label>Default Chat Model</Label>
              <Combobox
                items={modelOptions}
                selectedValue={form.chatModel}
                onSelectedValueChange={(value) =>
                  setForm((current) =>
                    current ? { ...current, chatModel: value ?? null } : current
                  )
                }
              >
                <ComboboxInput
                  placeholder="Search and select chat model"
                  showClear
                />
                <ComboboxContent className="max-h-80">
                  <ComboboxEmpty>No models found.</ComboboxEmpty>
                  <ComboboxList>
                    {(item) => (
                      <ComboboxItem key={item} value={item}>
                        {item}
                      </ComboboxItem>
                    )}
                  </ComboboxList>
                </ComboboxContent>
              </Combobox>
            </div>

            <div className="space-y-1">
              <Label>Default Audio Model</Label>
              <Combobox
                items={modelOptions}
                selectedValue={form.audioModel}
                onSelectedValueChange={(value) =>
                  setForm((current) =>
                    current
                      ? { ...current, audioModel: value ?? null }
                      : current
                  )
                }
              >
                <ComboboxInput
                  placeholder="Search and select audio model"
                  showClear
                />
                <ComboboxContent className="max-h-80">
                  <ComboboxEmpty>No models found.</ComboboxEmpty>
                  <ComboboxList>
                    {(item) => (
                      <ComboboxItem key={item} value={item}>
                        {item}
                      </ComboboxItem>
                    )}
                  </ComboboxList>
                </ComboboxContent>
              </Combobox>
            </div>

            <div className="flex items-center gap-2">
              <Button
                type="button"
                onClick={() => void handleSave()}
                disabled={isSaving}
              >
                {isSaving ? (
                  <LoaderCircle className="size-4 animate-spin" />
                ) : (
                  <Save className="size-4" />
                )}
                Save Provider
              </Button>
              <Button
                type="button"
                variant="outline"
                onClick={() =>
                  setForm((current) =>
                    current
                      ? { ...current, enabled: !current.enabled }
                      : current
                  )
                }
              >
                {form.enabled ? "Disable" : "Enable"}
              </Button>
            </div>
          </section>
        )}
      </SectionContent>
    </Section>
  );
}
