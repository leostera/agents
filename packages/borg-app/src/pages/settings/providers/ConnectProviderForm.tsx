import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
} from "@borg/ui";
import { Icon } from "@iconify/react";
import { KeyRound, LoaderCircle } from "lucide-react";
import React from "react";

export type ConnectProviderInput = {
  providerId: string;
  providerKind: "openai" | "openrouter" | "lmstudio" | "ollama";
  apiKey?: string;
  baseUrl?: string;
};

type ConnectProviderFormProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  isStartingOpenAi: boolean;
  isSaving: boolean;
  onStartOpenAiSignIn: () => void;
  onSave: (input: ConnectProviderInput) => void;
};

function isLocalProvider(provider: ConnectProviderInput["providerKind"]): boolean {
  return provider === "lmstudio" || provider === "ollama";
}

export function ConnectProviderForm({
  open,
  onOpenChange,
  isStartingOpenAi,
  isSaving,
  onStartOpenAiSignIn,
  onSave,
}: ConnectProviderFormProps) {
  const [dialogStep, setDialogStep] = React.useState<"provider" | "method">(
    "provider"
  );
  const [selectedProvider, setSelectedProvider] = React.useState<
    ConnectProviderInput["providerKind"]
  >("openai");
  const [providerId, setProviderId] = React.useState("openai");
  const [apiKey, setApiKey] = React.useState("");
  const [baseUrl, setBaseUrl] = React.useState("");

  React.useEffect(() => {
    if (!open) {
      setDialogStep("provider");
      setSelectedProvider("openai");
      setProviderId("openai");
      setApiKey("");
      setBaseUrl("");
    }
  }, [open]);

  const ProviderLogo = ({
    provider,
    className,
  }: {
    provider: ConnectProviderInput["providerKind"];
    className?: string;
  }) => {
    if (provider === "openai") {
      return (
        <Icon
          icon="streamline-logos:openai-logo-solid"
          className={`size-6 shrink-0 ${className ?? ""}`}
        />
      );
    }
    if (provider === "openrouter") {
      return (
        <Icon
          icon="simple-icons:openrouter"
          className={`size-6 shrink-0 ${className ?? ""}`}
        />
      );
    }
    if (provider === "ollama") {
      return (
        <Icon
          icon="simple-icons:ollama"
          className={`size-6 shrink-0 ${className ?? ""}`}
        />
      );
    }
    return (
      <Icon
        icon="mdi:desktop-tower-monitor"
        className={`size-6 shrink-0 ${className ?? ""}`}
      />
    );
  };

  const providerLabel =
    selectedProvider === "openai"
      ? "OpenAI"
      : selectedProvider === "openrouter"
        ? "OpenRouter"
        : selectedProvider === "lmstudio"
          ? "LM Studio"
          : "Ollama";

  const requiresApiKey = !isLocalProvider(selectedProvider);
  const requiresBaseUrl = isLocalProvider(selectedProvider);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Connect Provider</DialogTitle>
          <DialogDescription>
            Configure cloud or local providers for model access.
          </DialogDescription>
        </DialogHeader>

        {dialogStep === "provider" ? (
          <div className="space-y-2">
            <Label>Provider</Label>
            <div className="grid grid-cols-2 gap-2">
              {(["openai", "openrouter", "lmstudio", "ollama"] as const).map(
                (provider) => (
                  <Button
                    key={provider}
                    type="button"
                    variant="outline"
                    className="h-16 items-center justify-start gap-3"
                    onClick={() => {
                      setSelectedProvider(provider);
                      setProviderId(provider);
                      if (provider === "lmstudio") setBaseUrl("http://127.0.0.1:1234");
                      if (provider === "ollama") setBaseUrl("http://127.0.0.1:11434");
                      setDialogStep("method");
                    }}
                  >
                    <ProviderLogo provider={provider} />
                    <span className="text-sm font-medium">
                      {provider === "openai"
                        ? "OpenAI"
                        : provider === "openrouter"
                          ? "OpenRouter"
                          : provider === "lmstudio"
                            ? "LM Studio"
                            : "Ollama"}
                    </span>
                  </Button>
                )
              )}
            </div>
          </div>
        ) : (
          <form
            className="space-y-3"
            onSubmit={(event) => {
              event.preventDefault();
              onSave({
                providerId: providerId.trim(),
                providerKind: selectedProvider,
                apiKey: apiKey.trim() || undefined,
                baseUrl: baseUrl.trim() || undefined,
              });
            }}
          >
            <div className="space-y-2">
              <Label className="flex items-center gap-2">
                <ProviderLogo provider={selectedProvider} />
                {providerLabel}
              </Label>
              {selectedProvider === "openai" ? (
                <Button
                  type="button"
                  variant="outline"
                  onClick={onStartOpenAiSignIn}
                  disabled={isStartingOpenAi}
                >
                  {isStartingOpenAi ? (
                    <LoaderCircle className="size-4 animate-spin" />
                  ) : (
                    <KeyRound className="size-4" />
                  )}
                  Sign in with OpenAI
                </Button>
              ) : null}
            </div>

            {requiresApiKey ? (
              <div className="space-y-2">
                <Label htmlFor="provider-api-key">API Key</Label>
                <Input
                  id="provider-api-key"
                  type="password"
                  autoComplete="off"
                  value={apiKey}
                  onChange={(event) => setApiKey(event.currentTarget.value)}
                  placeholder={
                    selectedProvider === "openai" ? "sk-..." : "sk-or-v1-..."
                  }
                  required
                />
              </div>
            ) : null}

            <div className="space-y-2">
              <Label htmlFor="provider-id">Provider ID</Label>
              <Input
                id="provider-id"
                autoComplete="off"
                value={providerId}
                onChange={(event) => setProviderId(event.currentTarget.value)}
                placeholder="lmstudio-local"
                required
              />
            </div>

            {requiresBaseUrl ? (
              <div className="space-y-2">
                <Label htmlFor="provider-base-url">Base URL</Label>
                <Input
                  id="provider-base-url"
                  autoComplete="off"
                  value={baseUrl}
                  onChange={(event) => setBaseUrl(event.currentTarget.value)}
                  placeholder="http://127.0.0.1:1234"
                  required
                />
              </div>
            ) : null}

            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() => setDialogStep("provider")}
              >
                Back
              </Button>
              <Button type="submit" disabled={isSaving}>
                {isSaving ? (
                  <LoaderCircle className="size-4 animate-spin" />
                ) : null}
                Save Provider
              </Button>
            </div>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}
