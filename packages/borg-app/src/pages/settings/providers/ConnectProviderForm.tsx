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

type ConnectProviderFormProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  isStartingOpenAi: boolean;
  isSavingOpenAi: boolean;
  isSavingOpenRouter: boolean;
  openAiApiKey: string;
  openRouterApiKey: string;
  onOpenAiApiKeyChange: (value: string) => void;
  onOpenRouterApiKeyChange: (value: string) => void;
  onStartOpenAiSignIn: () => void;
  onSaveOpenAi: (event: React.FormEvent<HTMLFormElement>) => void;
  onSaveOpenRouter: (event: React.FormEvent<HTMLFormElement>) => void;
};

export function ConnectProviderForm({
  open,
  onOpenChange,
  isStartingOpenAi,
  isSavingOpenAi,
  isSavingOpenRouter,
  openAiApiKey,
  openRouterApiKey,
  onOpenAiApiKeyChange,
  onOpenRouterApiKeyChange,
  onStartOpenAiSignIn,
  onSaveOpenAi,
  onSaveOpenRouter,
}: ConnectProviderFormProps) {
  const [dialogStep, setDialogStep] = React.useState<"provider" | "method">(
    "provider"
  );
  const [selectedProvider, setSelectedProvider] = React.useState<
    "openai" | "openrouter"
  >("openai");

  React.useEffect(() => {
    if (!open) {
      setDialogStep("provider");
      setSelectedProvider("openai");
    }
  }, [open]);

  const showOpenAiApiKeyForm =
    dialogStep === "method" && selectedProvider === "openai";
  const showOpenRouterApiKeyForm =
    dialogStep === "method" && selectedProvider === "openrouter";

  const ProviderLogo = ({
    provider,
    className,
  }: {
    provider: "openai" | "openrouter";
    className?: string;
  }) =>
    provider === "openai" ? (
      <Icon
        icon="streamline-logos:openai-logo-solid"
        className={`size-6 shrink-0 ${className ?? ""}`}
      />
    ) : (
      <Icon
        icon="simple-icons:openrouter"
        className={`size-6 shrink-0 ${className ?? ""}`}
      />
    );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Connect Provider</DialogTitle>
          <DialogDescription>
            Choose how you want to connect OpenAI or OpenRouter.
          </DialogDescription>
        </DialogHeader>

        {dialogStep === "provider" ? (
          <div className="space-y-2">
            <Label>Provider</Label>
            <div className="grid grid-cols-2 gap-2">
              <Button
                type="button"
                variant="outline"
                className="h-16 items-center justify-start gap-3"
                onClick={() => {
                  setSelectedProvider("openai");
                  setDialogStep("method");
                }}
              >
                <ProviderLogo provider="openai" />
                <span className="text-sm font-medium">OpenAI</span>
              </Button>
              <Button
                type="button"
                variant="outline"
                className="h-16 items-center justify-start gap-3"
                onClick={() => {
                  setSelectedProvider("openrouter");
                  setDialogStep("method");
                }}
              >
                <ProviderLogo provider="openrouter" />
                <span className="text-sm font-medium">OpenRouter</span>
              </Button>
            </div>
          </div>
        ) : (
          <form
            className="space-y-3"
            onSubmit={
              selectedProvider === "openai" ? onSaveOpenAi : onSaveOpenRouter
            }
          >
            <div className="space-y-2">
              <Label className="flex items-center gap-2">
                <ProviderLogo provider={selectedProvider} />
                {selectedProvider === "openai" ? "OpenAI" : "OpenRouter"}
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

            {showOpenAiApiKeyForm || showOpenRouterApiKeyForm ? (
              <div className="space-y-2">
                <Label
                  htmlFor={
                    selectedProvider === "openai"
                      ? "openai-api-key"
                      : "openrouter-api-key"
                  }
                >
                  API Key
                </Label>
                <Input
                  id={
                    selectedProvider === "openai"
                      ? "openai-api-key"
                      : "openrouter-api-key"
                  }
                  type="password"
                  autoComplete="off"
                  value={
                    selectedProvider === "openai"
                      ? openAiApiKey
                      : openRouterApiKey
                  }
                  onChange={(event) => {
                    if (selectedProvider === "openai") {
                      onOpenAiApiKeyChange(event.currentTarget.value);
                      return;
                    }
                    onOpenRouterApiKeyChange(event.currentTarget.value);
                  }}
                  placeholder={
                    selectedProvider === "openai" ? "sk-..." : "sk-or-v1-..."
                  }
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
              <Button
                type="submit"
                disabled={
                  selectedProvider === "openai"
                    ? isSavingOpenAi
                    : isSavingOpenRouter
                }
              >
                {(
                  selectedProvider === "openai"
                    ? isSavingOpenAi
                    : isSavingOpenRouter
                ) ? (
                  <LoaderCircle className="size-4 animate-spin" />
                ) : null}
                Save API Key
              </Button>
            </div>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}
