import { createBorgApiClient } from "@borg/api";
import { createI18n } from "@borg/i18n";
import {
  Button,
  ChatComposerShell,
  type ChatMessageItem,
  ChatThread,
} from "@borg/ui";
import { Icon } from "@iconify/react";
import { MonitorSmartphone } from "lucide-react";
import React from "react";

type AiMode = "openai" | "openrouter" | "local";
type Channel = "telegram" | "discord";
type Step =
  | "chooseMode"
  | "enterApiKey"
  | "chooseChannel"
  | "channelToken"
  | "testMessage"
  | "complete";

type SetupState = {
  mode: AiMode | null;
  providerId: string | null;
  assistantName: string;
  behaviorId: string | null;
  actorId: string | null;
  actorDisplayName: string | null;
  channel: Channel | null;
  portId: string | null;
  portName: string | null;
  telegramHandle: string | null;
};

type OnboardingChatRuntime = {
  actorId: string;
  behaviorId: string;
  sessionId: string;
  userId: string;
};

type TelegramBotInfo = {
  handle: string | null;
  displayName: string | null;
};

const borgApi = createBorgApiClient();
const i18n = createI18n("en");

const DEFAULT_ASSISTANT_PROMPT =
  "You are a personal assistant that helps with daily tasks. Be concise, practical, and friendly. Ask one clarifying question when needed before acting.";

const ONBOARDING_ACTOR_PROMPT = `You are Borg's onboarding assistant. Keep replies short, friendly, and practical.
The product is local-first and the user is likely non-technical.
Goals:
1) Help user create their first assistant.
2) Help user connect a channel (Telegram recommended, Discord optional).
3) Help user send a test 'hi' and confirm success.
Rules:
- Do not ask for internal IDs.
- Prefer one clear next action per reply.
- When asked to react to a system update, acknowledge and guide the next step.
- Keep responses under 4 short lines when possible.`;

function nowTimestamp(): string {
  return new Date().toLocaleTimeString();
}

function createLocalId(prefix: string): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return `${prefix}:${crypto.randomUUID()}`;
  }
  return `${prefix}:${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`;
}

function toSlug(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 32);
}

function channelLabel(channel: Channel): string {
  return channel === "telegram"
    ? i18n.t("onboard.channel.telegram")
    : i18n.t("onboard.channel.discord");
}

function modeLabel(mode: AiMode): string {
  if (mode === "openai") return i18n.t("onboard.mode.openai");
  if (mode === "openrouter") return i18n.t("onboard.mode.openrouter");
  return i18n.t("onboard.mode.local");
}

function ModeIcon({ mode }: { mode: AiMode }) {
  if (mode === "openai") {
    return (
      <Icon
        icon="streamline-logos:openai-logo-solid"
        className="size-5"
        aria-hidden="true"
      />
    );
  }
  if (mode === "openrouter") {
    return (
      <Icon
        icon="simple-icons:openrouter"
        className="size-5"
        aria-hidden="true"
      />
    );
  }
  return <MonitorSmartphone className="size-5" aria-hidden="true" />;
}

function maskSecret(value: string): string {
  if (!value.trim()) return "••••";
  return "•".repeat(Math.max(6, Math.min(16, value.trim().length)));
}

function telegramStartLink(handle: string | null): string | null {
  if (!handle) return null;
  const username = handle.replace(/^@/, "").trim();
  if (!username) return null;
  return `tg://resolve?domain=${encodeURIComponent(username)}&start=hi`;
}

type MirroredChatMessage = {
  messageIdentity: string;
  role: "assistant" | "user" | "system";
  text: string;
  timestamp: string;
};

function detectMessageRole(
  payload: Record<string, unknown>
): "assistant" | "user" | "system" {
  const typeCandidate = payload.type;
  if (typeof typeCandidate === "string") {
    const type = typeCandidate.trim().toLowerCase();
    if (type === "assistant") return "assistant";
    if (type === "user") return "user";
    if (type === "system") return "system";
    if (
      type === "tool_call" ||
      type === "tool_result" ||
      type === "session_event"
    ) {
      return "system";
    }
  }

  const roleCandidate =
    typeof payload.role === "string"
      ? payload.role.trim().toLowerCase()
      : typeof payload.author === "string"
        ? payload.author.trim().toLowerCase()
        : null;
  if (roleCandidate === "assistant") return "assistant";
  if (roleCandidate === "user") return "user";
  return "system";
}

function extractMessageText(payload: Record<string, unknown>): string {
  if (typeof payload.content === "string" && payload.content.trim()) {
    return payload.content;
  }
  if (typeof payload.text === "string" && payload.text.trim()) {
    return payload.text;
  }
  return "";
}

function isChatPayload(payload: Record<string, unknown>): boolean {
  const typeCandidate = payload.type;
  if (typeof typeCandidate === "string") {
    const type = typeCandidate.trim().toLowerCase();
    if (type === "assistant" || type === "user") return true;
    if (
      type === "system" ||
      type === "tool_call" ||
      type === "tool_result" ||
      type === "session_event"
    ) {
      return false;
    }
  }
  const roleCandidate =
    typeof payload.role === "string"
      ? payload.role.trim().toLowerCase()
      : typeof payload.author === "string"
        ? payload.author.trim().toLowerCase()
        : null;
  if (roleCandidate === "assistant" || roleCandidate === "user") return true;
  return false;
}

function formatTimestamp(value: unknown): string {
  if (typeof value !== "string" || !value.trim()) return nowTimestamp();
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return nowTimestamp();
  return date.toLocaleTimeString();
}

function toMirroredChatMessages(
  rawMessages: Record<string, unknown>[]
): MirroredChatMessage[] {
  return rawMessages
    .filter((raw) => isChatPayload(raw as Record<string, unknown>))
    .map((raw, index) => {
      const payload = raw as Record<string, unknown>;
      const role = detectMessageRole(payload);
      const text = extractMessageText(payload);
      const timestamp = formatTimestamp(
        typeof payload.created_at === "string"
          ? payload.created_at
          : typeof payload.timestamp === "string"
            ? payload.timestamp
            : payload.updated_at
      );
      const messageIdentity =
        (typeof payload.message_id === "string" && payload.message_id.trim()) ||
        `${role}|${text}|${timestamp}|${index}`;
      return { messageIdentity, role, text, timestamp };
    })
    .filter((item) => item.text.trim().length > 0);
}

async function fetchTelegramBotInfo(token: string): Promise<TelegramBotInfo> {
  try {
    const response = await fetch(
      `https://api.telegram.org/bot${encodeURIComponent(token)}/getMe`
    );
    if (!response.ok) return { handle: null, displayName: null };
    const payload = (await response.json()) as {
      ok?: boolean;
      result?: { username?: string; first_name?: string };
    };
    if (!payload.ok || !payload.result)
      return { handle: null, displayName: null };
    const handle = payload.result.username
      ? `@${payload.result.username}`
      : null;
    const displayName =
      payload.result.first_name?.trim() ||
      payload.result.username?.trim() ||
      null;
    return { handle, displayName };
  } catch {
    return { handle: null, displayName: null };
  }
}

export function OnboardApp() {
  const [messages, setMessages] = React.useState<ChatMessageItem[]>([]);
  const [step, setStep] = React.useState<Step>("chooseMode");
  const [draft, setDraft] = React.useState("");
  const [isSubmitting, setIsSubmitting] = React.useState(false);
  const [inlineError, setInlineError] = React.useState<string | null>(null);
  const [runtime, setRuntime] = React.useState<OnboardingChatRuntime | null>(
    null
  );
  const submitInFlightRef = React.useRef(false);
  const pollInFlightRef = React.useRef(false);
  const mirroredCursorRef = React.useRef(0);
  const mirroredSessionRef = React.useRef<string | null>(null);
  const [mirroredSessionId, setMirroredSessionId] = React.useState<
    string | null
  >(null);
  const [setup, setSetup] = React.useState<SetupState>({
    mode: null,
    providerId: null,
    assistantName: "",
    behaviorId: null,
    actorId: null,
    actorDisplayName: null,
    channel: null,
    portId: null,
    portName: null,
    telegramHandle: null,
  });

  const appendMessage = React.useCallback(
    (message: Omit<ChatMessageItem, "id" | "timestamp">) => {
      const next: ChatMessageItem = {
        id: `onboard-${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`,
        timestamp: nowTimestamp(),
        ...message,
      };
      setMessages((current) => [...current, next]);
      return next.id;
    },
    []
  );

  React.useEffect(() => {
    const timeoutId = window.setTimeout(() => {
      setMessages((current) => {
        if (current.some((message) => message.id === "onboard-intro")) {
          return current;
        }
        return [
          ...current,
          {
            id: "onboard-intro",
            role: "assistant",
            text: i18n.t("onboard.assistant.intro_single"),
            timestamp: nowTimestamp(),
          },
        ];
      });
    }, 520);
    return () => window.clearTimeout(timeoutId);
  }, []);

  const patchMessage = React.useCallback(
    (id: string, patch: Partial<ChatMessageItem>) => {
      setMessages((current) =>
        current.map((message) =>
          message.id === id
            ? {
                ...message,
                ...patch,
                timestamp:
                  patch.timestamp ?? message.timestamp ?? nowTimestamp(),
              }
            : message
        )
      );
    },
    []
  );

  const sendActorTurn = React.useCallback(
    async ({
      text,
      asUser = true,
      metadata,
      showFailureMessage = true,
    }: {
      text: string;
      asUser?: boolean;
      metadata?: Record<string, unknown>;
      showFailureMessage?: boolean;
    }): Promise<string> => {
      if (!runtime) return "";

      if (asUser) {
        appendMessage({ role: "user", text });
      }

      const pendingId = appendMessage({
        role: "assistant",
        text: "...",
        pending: true,
      });

      try {
        const response = await borgApi.chatActor({
          actorId: runtime.actorId,
          sessionId: runtime.sessionId,
          userId: runtime.userId,
          text,
          metadata: metadata ?? {
            source: "onboard",
            step,
          },
        });
        const reply = response.reply?.trim() || "";
        patchMessage(pendingId, {
          text: reply || i18n.t("onboard.assistant.retry_test_message"),
          pending: false,
        });
        return reply;
      } catch {
        if (showFailureMessage) {
          patchMessage(pendingId, {
            role: "system",
            text: "I had trouble responding. Please try again.",
            pending: false,
          });
        } else {
          setMessages((current) =>
            current.filter((message) => message.id !== pendingId)
          );
        }
        return "";
      }
    },
    [appendMessage, patchMessage, runtime, step]
  );

  const bootstrapOnboardingActor = React.useCallback(
    async (mode: AiMode, providerId: string | null) => {
      const behaviorId = createLocalId("borg:behavior:onboard");
      const actorId = createLocalId("borg:actor:onboard");
      const sessionId = createLocalId("borg:session:onboard");
      const userId = createLocalId("borg:user:onboard");

      await borgApi.upsertBehavior({
        behaviorId,
        name: "Onboarding Assistant",
        systemPrompt: ONBOARDING_ACTOR_PROMPT,
        preferredProviderId: providerId,
        requiredCapabilitiesJson: [],
        sessionTurnConcurrency: "serial",
        status: "ACTIVE",
      });

      await borgApi.upsertActor({
        actorId,
        name: "Onboarding Assistant",
        systemPrompt: ONBOARDING_ACTOR_PROMPT,
        defaultBehaviorId: behaviorId,
        status: "RUNNING",
      });

      const nextRuntime = { actorId, behaviorId, sessionId, userId };
      setRuntime(nextRuntime);

      setStep("chooseChannel");
      setInlineError(null);
      setDraft("");

      // initial onboarding turn
      const pendingId = appendMessage({
        role: "assistant",
        text: "...",
        pending: true,
      });
      try {
        const response = await borgApi.chatActor({
          actorId,
          sessionId,
          userId,
          text:
            mode === "local"
              ? "Local mode is enabled. Ask the user to choose a channel next. Recommend Telegram first."
              : `Provider ${modeLabel(mode)} is connected. Ask the user to choose a channel next. Recommend Telegram first.`,
          metadata: {
            source: "onboard",
            step: "chooseChannel",
            mode,
          },
        });
        patchMessage(pendingId, {
          text:
            response.reply?.trim() ||
            i18n.t("onboard.assistant.choose_channel"),
          pending: false,
        });
      } catch {
        patchMessage(pendingId, {
          text: i18n.t("onboard.assistant.choose_channel"),
          pending: false,
        });
      }
    },
    [appendMessage, patchMessage]
  );

  const handleModeSelection = React.useCallback(
    async (mode: AiMode) => {
      if (isSubmitting || step !== "chooseMode") return;
      setInlineError(null);
      setSetup((current) => ({ ...current, mode }));
      appendMessage({ role: "user", text: modeLabel(mode) });

      if (mode === "local") {
        setIsSubmitting(true);
        appendMessage({
          role: "assistant",
          text: i18n.t("onboard.assistant.local_mode_selected"),
        });
        try {
          await bootstrapOnboardingActor(mode, null);
        } catch {
          appendMessage({
            role: "system",
            text: i18n.t("onboard.error.bootstrap_failed"),
          });
          setStep("chooseMode");
        } finally {
          setIsSubmitting(false);
        }
        return;
      }

      appendMessage({
        role: "assistant",
        text: i18n.t("onboard.assistant.ask_api_key", {
          provider: modeLabel(mode),
        }),
      });
      setStep("enterApiKey");
      setDraft("");
    },
    [appendMessage, bootstrapOnboardingActor, isSubmitting, step]
  );

  const handleApiKeySubmit = React.useCallback(
    async (submitted?: string) => {
      const apiKey = (submitted ?? draft).trim();
      if (!apiKey) {
        setInlineError(i18n.t("onboard.error.api_key_required"));
        return;
      }
      if (setup.mode !== "openai" && setup.mode !== "openrouter") {
        setInlineError(i18n.t("onboard.error.mode_missing"));
        return;
      }

      setInlineError(null);
      setIsSubmitting(true);
      appendMessage({ role: "user", text: maskSecret(apiKey) });
      const pendingId = appendMessage({
        role: "assistant",
        text: i18n.t("onboard.assistant.checking_credentials"),
        pending: true,
      });

      const providerId = createLocalId("borg:provider");
      try {
        await borgApi.upsertProvider({
          provider: providerId,
          providerKind: setup.mode,
          apiKey,
          enabled: true,
        });
        await borgApi.listProviderModels(providerId);
        setSetup((current) => ({ ...current, providerId }));
        patchMessage(pendingId, {
          text: i18n.t("onboard.assistant.provider_connected", {
            provider: modeLabel(setup.mode as "openai" | "openrouter"),
          }),
          pending: false,
        });
        await bootstrapOnboardingActor(setup.mode, providerId);
      } catch {
        await borgApi.deleteProvider(providerId, { ignoreNotFound: true });
        patchMessage(pendingId, {
          text: i18n.t("onboard.error.invalid_api_key"),
          pending: false,
          role: "system",
        });
        setInlineError(i18n.t("onboard.error.invalid_api_key_retry"));
      } finally {
        setIsSubmitting(false);
        setDraft("");
      }
    },
    [appendMessage, bootstrapOnboardingActor, draft, patchMessage, setup.mode]
  );

  const handleChannelSelection = React.useCallback(
    async (channel: Channel) => {
      if (isSubmitting || step !== "chooseChannel") return;
      setInlineError(null);
      setIsSubmitting(true);
      setSetup((current) => ({ ...current, channel }));
      appendMessage({ role: "user", text: channelLabel(channel) });

      setStep("channelToken");
      setDraft("");
      appendMessage({
        role: "assistant",
        text:
          channel === "telegram"
            ? i18n.t("onboard.assistant.ask_telegram_token")
            : i18n.t("onboard.assistant.ask_discord_token"),
      });

      setIsSubmitting(false);
    },
    [appendMessage, isSubmitting, step]
  );

  const handleChannelTokenSubmit = React.useCallback(
    async (submitted?: string) => {
      const token = (submitted ?? draft).trim();
      const { channel } = setup;
      if (!channel) {
        setInlineError(i18n.t("onboard.error.channel_missing"));
        return;
      }
      if (!token) {
        setInlineError(
          channel === "telegram"
            ? i18n.t("onboard.error.telegram_token_required")
            : i18n.t("onboard.error.discord_token_required")
        );
        return;
      }

      if (
        channel === "telegram" &&
        !/^\d{6,}:[A-Za-z0-9_-]{20,}$/.test(token)
      ) {
        setInlineError(i18n.t("onboard.error.telegram_token_invalid"));
        appendMessage({
          role: "assistant",
          text: i18n.t("onboard.assistant.telegram_help"),
        });
        return;
      }
      if (channel === "discord" && token.length < 20) {
        setInlineError(i18n.t("onboard.error.discord_token_invalid"));
        return;
      }

      setInlineError(null);
      setIsSubmitting(true);
      appendMessage({ role: "user", text: maskSecret(token) });

      const botInfo =
        channel === "telegram"
          ? await fetchTelegramBotInfo(token)
          : { handle: null, displayName: null };
      const assistantName =
        botInfo.displayName || setup.assistantName || "Assistant";
      const slug = toSlug(assistantName) || "assistant";
      const behaviorId = setup.behaviorId ?? createLocalId("borg:behavior");
      const actorId = setup.actorId ?? `borg:actor:${slug}-01`;
      const actorDisplayName = setup.actorDisplayName ?? `${assistantName}-01`;
      const unique = `${Date.now()}`.slice(-6);
      const portName = `${slug}-${channel}-${unique}`;
      const portId = `borg:port:${portName}`;

      try {
        if (!setup.actorId || !setup.behaviorId) {
          await borgApi.upsertBehavior({
            behaviorId,
            name: assistantName,
            systemPrompt: DEFAULT_ASSISTANT_PROMPT,
            preferredProviderId: setup.providerId,
            requiredCapabilitiesJson: [],
            sessionTurnConcurrency: "serial",
            status: "ACTIVE",
          });

          await borgApi.upsertActor({
            actorId,
            name: actorDisplayName,
            systemPrompt: DEFAULT_ASSISTANT_PROMPT,
            defaultBehaviorId: behaviorId,
            status: "RUNNING",
          });
        }

        const settings: Record<string, unknown> = { bot_token: token };
        if (channel === "telegram" || channel === "discord") {
          settings.allowed_external_user_ids = [];
        }

        await borgApi.upsertPort(portId, {
          provider: channel,
          enabled: true,
          allows_guests: true,
          assigned_actor_id: actorId,
          settings,
        });

        const telegramHandle = channel === "telegram" ? botInfo.handle : null;

        setSetup((current) => ({
          ...current,
          assistantName,
          behaviorId,
          actorId,
          actorDisplayName,
          portId,
          portName,
          telegramHandle,
        }));

        setStep("testMessage");
        setDraft("");
        setMirroredSessionId(null);
        mirroredCursorRef.current = 0;
        mirroredSessionRef.current = null;
        const botName = botInfo.displayName ?? botInfo.handle ?? "your bot";
        if (channel === "telegram") {
          const startLink = telegramStartLink(telegramHandle);
          appendMessage({
            role: "assistant",
            text: startLink
              ? i18n.t("onboard.assistant.telegram_ready", {
                  botName,
                  link: startLink,
                })
              : i18n.t("onboard.assistant.telegram_ready_no_link", { botName }),
          });
        } else {
          appendMessage({
            role: "assistant",
            text: i18n.t("onboard.assistant.say_hi_prompt", {
              channel: channelLabel(channel),
            }),
          });
        }
      } catch {
        appendMessage({
          role: "system",
          text:
            channel === "telegram"
              ? i18n.t("onboard.error.telegram_connect_failed")
              : i18n.t("onboard.error.discord_connect_failed"),
        });
        setInlineError(
          channel === "telegram"
            ? i18n.t("onboard.error.telegram_connect_retry")
            : i18n.t("onboard.error.discord_connect_retry")
        );
      } finally {
        setIsSubmitting(false);
      }
    },
    [appendMessage, draft, setup]
  );

  React.useEffect(() => {
    const portId = setup.portId;
    const actorId = setup.actorId;
    if (
      (step !== "testMessage" && step !== "complete") ||
      setup.channel !== "telegram" ||
      !portId ||
      !actorId
    ) {
      return;
    }

    let active = true;

    const poll = async () => {
      if (!active || pollInFlightRef.current) return;
      pollInFlightRef.current = true;
      try {
        let sessionId = mirroredSessionId;
        if (!sessionId) {
          const bindings = await borgApi.listPortBindings(portId, 50);
          const match =
            bindings.find((binding) => binding.agent_id === actorId) ??
            bindings[0];
          sessionId = match?.session_id ?? null;
          if (sessionId) {
            setMirroredSessionId(sessionId);
          }
        }
        if (!sessionId) return;
        if (mirroredSessionRef.current !== sessionId) {
          mirroredSessionRef.current = sessionId;
          mirroredCursorRef.current = 0;
        }

        const rawMessages = await borgApi.listSessionMessages(sessionId, {
          from: 0,
          limit: 250,
        });
        if (!active || rawMessages.length === 0) return;

        const fromIndex = Math.min(
          mirroredCursorRef.current,
          rawMessages.length
        );
        const nextBatch = rawMessages.slice(fromIndex);
        mirroredCursorRef.current = rawMessages.length;
        const mapped = toMirroredChatMessages(nextBatch);
        let sawAssistant = false;

        for (const item of mapped) {
          if (item.role === "assistant") {
            sawAssistant = true;
          }
          setMessages((current) => [
            ...current,
            {
              id: `onboard-mirror-${item.messageIdentity}`,
              role: item.role,
              text: item.text,
              timestamp: item.timestamp,
            },
          ]);
        }

        if (sawAssistant && step === "testMessage") {
          setStep("complete");
        }
      } catch {
        // Keep polling silently during onboarding handoff.
      } finally {
        pollInFlightRef.current = false;
      }
    };

    void poll();
    const interval = window.setInterval(() => {
      void poll();
    }, 1500);

    return () => {
      active = false;
      window.clearInterval(interval);
    };
  }, [mirroredSessionId, setup.actorId, setup.channel, setup.portId, step]);

  const submitDraft = React.useCallback(async () => {
    if (isSubmitting || submitInFlightRef.current) return;
    submitInFlightRef.current = true;
    try {
      const submitted = draft;
      setDraft("");
      if (step === "enterApiKey") {
        await handleApiKeySubmit(submitted);
        return;
      }
      if (step === "channelToken") {
        await handleChannelTokenSubmit(submitted);
      }
    } finally {
      submitInFlightRef.current = false;
    }
  }, [draft, handleApiKeySubmit, handleChannelTokenSubmit, isSubmitting, step]);

  const canGoBack = step !== "chooseMode" && step !== "complete";
  const handleBack = React.useCallback(() => {
    setInlineError(null);
    setDraft("");
    if (step === "enterApiKey") {
      setStep("chooseMode");
      return;
    }
    if (step === "channelToken") {
      setStep("chooseChannel");
      return;
    }
    if (step === "testMessage") {
      setStep("channelToken");
    }
  }, [step]);

  const composer = React.useMemo(() => {
    if (step === "chooseMode") {
      return (
        <div className="rounded-xl border bg-background p-3">
          <p className="mb-2 text-muted-foreground text-xs">
            {i18n.t("onboard.composer.choose_mode")}
          </p>
          <div className="flex gap-2 overflow-x-auto">
            <Button
              variant="outline"
              onClick={() => void handleModeSelection("openai")}
              disabled={isSubmitting}
              className="h-auto shrink-0 justify-start gap-3 px-4 py-3 text-left"
            >
              <ModeIcon mode="openai" />
              <span>{i18n.t("onboard.mode.openai")}</span>
            </Button>
            <Button
              variant="outline"
              onClick={() => void handleModeSelection("openrouter")}
              disabled={isSubmitting}
              className="h-auto shrink-0 justify-start gap-3 px-4 py-3 text-left"
            >
              <ModeIcon mode="openrouter" />
              <span>{i18n.t("onboard.mode.openrouter")}</span>
            </Button>
            <Button
              variant="outline"
              onClick={() => void handleModeSelection("local")}
              disabled={isSubmitting}
              className="h-auto shrink-0 justify-start gap-3 px-4 py-3 text-left"
            >
              <ModeIcon mode="local" />
              <span>{i18n.t("onboard.mode.local")}</span>
            </Button>
          </div>
        </div>
      );
    }

    if (step === "chooseChannel") {
      return (
        <div className="rounded-xl border bg-background p-3">
          <p className="mb-2 text-muted-foreground text-xs">
            {i18n.t("onboard.composer.choose_channel")}
          </p>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            <Button
              variant="outline"
              onClick={() => void handleChannelSelection("telegram")}
              disabled={isSubmitting}
            >
              {i18n.t("onboard.channel.telegram_recommended")}
            </Button>
            <Button
              variant="outline"
              onClick={() => void handleChannelSelection("discord")}
              disabled={isSubmitting}
            >
              {i18n.t("onboard.channel.discord")}
            </Button>
          </div>
        </div>
      );
    }

    if (step === "testMessage") {
      return (
        <div className="space-y-2 rounded-xl border bg-background p-3">
          <p className="text-sm">{i18n.t("onboard.test.instructions")}</p>
          {setup.telegramHandle ? (
            <p className="font-mono text-muted-foreground text-xs">
              {i18n.t("onboard.test.telegram_handle", {
                handle: setup.telegramHandle,
              })}
            </p>
          ) : null}
          {mirroredSessionId ? (
            <p className="text-muted-foreground text-xs">
              Waiting for your Telegram message...
            </p>
          ) : (
            <p className="text-muted-foreground text-xs">
              Waiting for your Telegram chat to start...
            </p>
          )}
        </div>
      );
    }

    if (step === "complete") {
      const providerStatus =
        setup.mode === "local"
          ? i18n.t("onboard.summary.provider_local")
          : i18n.t("onboard.summary.provider_connected", {
              provider:
                setup.mode === "openai"
                  ? i18n.t("onboard.mode.openai")
                  : i18n.t("onboard.mode.openrouter"),
            });

      return (
        <div className="space-y-2 rounded-xl border border-emerald-600/30 bg-emerald-600/10 p-3 text-sm">
          <p className="font-medium">{i18n.t("onboard.complete.title")}</p>
          <p>
            {i18n.t("onboard.summary.assistant", { name: setup.assistantName })}
          </p>
          <p>{providerStatus}</p>
          <p>
            {i18n.t("onboard.summary.channel", {
              channel: setup.channel ? channelLabel(setup.channel) : "-",
            })}
          </p>
          {setup.telegramHandle ? (
            <p>
              {i18n.t("onboard.summary.telegram_handle", {
                handle: setup.telegramHandle,
              })}
            </p>
          ) : null}
          <p className="text-xs">{i18n.t("onboard.complete.close_window")}</p>
        </div>
      );
    }

    const placeholder =
      step === "enterApiKey"
        ? i18n.t("onboard.placeholder.api_key")
        : i18n.t("onboard.placeholder.channel_token");

    return (
      <ChatComposerShell
        value={draft}
        onChange={setDraft}
        onSubmit={() => void submitDraft()}
        isRunning={isSubmitting}
        placeholder={placeholder}
      />
    );
  }, [
    appendMessage,
    draft,
    handleChannelSelection,
    handleModeSelection,
    isSubmitting,
    mirroredSessionId,
    setup.assistantName,
    setup.channel,
    setup.mode,
    setup.telegramHandle,
    step,
    submitDraft,
  ]);

  return (
    <section className="relative min-h-[100svh] overflow-hidden bg-background">
      <div className="pointer-events-none absolute -left-24 -top-24 h-72 w-72 rounded-full bg-cyan-400/15 blur-3xl" />
      <div className="pointer-events-none absolute -bottom-32 -right-20 h-80 w-80 rounded-full bg-emerald-400/10 blur-3xl" />
      <main className="mx-auto flex h-[100svh] w-full max-w-5xl min-h-0 flex-col px-4 py-4 sm:px-6 sm:py-6">
        <div className="min-h-0 flex-1 overflow-hidden rounded-3xl border border-border/70 bg-background/90 shadow-sm">
          <ChatThread
            messages={messages}
            isLoading={false}
            showEmptyState={false}
          >
            <div className="space-y-2">
              {canGoBack ? (
                <Button variant="outline" size="sm" onClick={handleBack}>
                  Back
                </Button>
              ) : null}
              {inlineError ? (
                <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-destructive text-xs">
                  {inlineError}
                </p>
              ) : null}
              {composer}
            </div>
          </ChatThread>
        </div>
      </main>
    </section>
  );
}
