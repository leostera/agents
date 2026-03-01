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
} from "@borg/ui";
import {
  AtSign,
  Globe,
  type LucideIcon,
  MessageCircle,
  Send,
  Smartphone,
} from "lucide-react";
import React from "react";

export type AddPortInput = {
  portKind: string;
  portName: string;
  telegramBotToken?: string;
  discordBotToken?: string;
};

type PortKindOption = {
  id: string;
  label: string;
  icon: LucideIcon;
};

const PORT_KIND_OPTIONS: PortKindOption[] = [
  { id: "telegram", label: "Telegram", icon: Send },
  { id: "whatsapp", label: "WhatsApp", icon: MessageCircle },
  { id: "x", label: "X.com", icon: AtSign },
  { id: "discord", label: "Discord", icon: MessageCircle },
  { id: "sms", label: "SMS", icon: Smartphone },
  { id: "http", label: "HTTP", icon: Globe },
];

type AddPortFormProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  isSaving: boolean;
  onSubmit: (input: AddPortInput) => Promise<void>;
};

export function AddPortForm({
  open,
  onOpenChange,
  isSaving,
  onSubmit,
}: AddPortFormProps) {
  const [dialogStep, setDialogStep] = React.useState<"kind" | "config">("kind");
  const [selectedPortKind, setSelectedPortKind] = React.useState("telegram");
  const [portName, setPortName] = React.useState("");
  const [telegramBotToken, setTelegramBotToken] = React.useState("");
  const [discordBotToken, setDiscordBotToken] = React.useState("");

  React.useEffect(() => {
    if (!open) {
      setDialogStep("kind");
      setSelectedPortKind("telegram");
      setPortName("");
      setTelegramBotToken("");
      setDiscordBotToken("");
    }
  }, [open]);

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    await onSubmit({
      portKind: selectedPortKind,
      portName,
      telegramBotToken,
      discordBotToken,
    });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Add Port</DialogTitle>
          <DialogDescription>
            Choose a port kind, then fill the required fields.
          </DialogDescription>
        </DialogHeader>
        {dialogStep === "kind" ? (
          <div className="space-y-2">
            <Label>Port kind</Label>
            <div className="grid grid-cols-2 gap-2">
              {PORT_KIND_OPTIONS.map((option) => {
                const Icon = option.icon;
                const isEnabled =
                  option.id === "telegram" || option.id === "discord";
                return (
                  <Button
                    key={option.id}
                    type="button"
                    variant="outline"
                    className="h-16 justify-start gap-3"
                    disabled={!isEnabled}
                    onClick={() => {
                      if (!isEnabled) return;
                      setSelectedPortKind(option.id);
                      setPortName(option.id);
                      setDialogStep("config");
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
        ) : (
          <form className="space-y-3" onSubmit={handleSubmit}>
            <div className="space-y-2">
              <Label htmlFor="port-name">Port name</Label>
              <Input
                id="port-name"
                value={portName}
                onChange={(event) => setPortName(event.currentTarget.value)}
                placeholder={selectedPortKind}
                aria-label="Port name"
              />
            </div>
            {selectedPortKind === "telegram" ? (
              <div className="space-y-2">
                <Label htmlFor="telegram-bot-token">Telegram bot token</Label>
                <Input
                  id="telegram-bot-token"
                  type="password"
                  autoComplete="off"
                  value={telegramBotToken}
                  onChange={(event) =>
                    setTelegramBotToken(event.currentTarget.value)
                  }
                  placeholder="123456:ABC..."
                  aria-label="Telegram bot token"
                />
                <div className="text-muted-foreground space-y-1 text-xs">
                  <p>How to get this token:</p>
                  <p>
                    1. Open Telegram and start a chat with{" "}
                    <code>@BotFather</code>.
                  </p>
                  <p>
                    2. Run <code>/newbot</code> and follow the prompts.
                  </p>
                  <p>
                    3. Copy the API token from BotFather and paste it here
                    (format: <code>123456:ABC...</code>).
                  </p>
                </div>
              </div>
            ) : null}
            {selectedPortKind === "discord" ? (
              <div className="space-y-2">
                <Label htmlFor="discord-bot-token">Discord bot token</Label>
                <Input
                  id="discord-bot-token"
                  type="password"
                  autoComplete="off"
                  value={discordBotToken}
                  onChange={(event) =>
                    setDiscordBotToken(event.currentTarget.value)
                  }
                  placeholder="Discord bot token"
                  aria-label="Discord bot token"
                />
                <div className="text-muted-foreground space-y-1 text-xs">
                  <p>How to get this token:</p>
                  <p>
                    1. Open the Discord Developer Portal and create/select an
                    application.
                  </p>
                  <p>
                    2. Add a Bot user under <code>Bot</code>.
                  </p>
                  <p>
                    3. Copy the bot token and paste it here.
                  </p>
                </div>
              </div>
            ) : null}
            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() => setDialogStep("kind")}
              >
                Back
              </Button>
              <Button type="submit" disabled={isSaving}>
                {isSaving ? "Saving..." : "Save Port"}
              </Button>
            </div>
          </form>
        )}
        <DialogFooter showCloseButton />
      </DialogContent>
    </Dialog>
  );
}
