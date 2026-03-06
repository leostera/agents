import {
  deleteProvider,
  listOnboardingPortBindingsByPortId,
  listOnboardingSessionMessages,
  listProviderModels,
  type OnboardingSessionMessage,
  upsertOnboardingActor,
  upsertOnboardingPort,
  upsertProvider,
} from "@borg/graphql-client";
import { createI18n } from "@borg/i18n";
import { createEventReducer, useStateReducer } from "@borg/react-statereducer";
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
  actorId: string | null;
  actorDisplayName: string | null;
  channel: Channel | null;
  portId: string | null;
  portName: string | null;
  telegramHandle: string | null;
};

type TelegramBotInfo = {
  handle: string | null;
  displayName: string | null;
};

type OnboardState = {
  messages: ChatMessageItem[];
  step: Step;
  draft: string;
  isSubmitting: boolean;
  inlineError: string | null;
  mirroredSessionId: string | null;
  setup: SetupState;
};

type OnboardEvent =
  | { type: "chat/append"; message: ChatMessageItem }
  | { type: "chat/append_many"; messages: ChatMessageItem[] }
  | { type: "chat/patch"; id: string; patch: Partial<ChatMessageItem> }
  | { type: "chat/remove"; id: string }
  | { type: "flow/set_step"; step: Step }
  | { type: "flow/set_draft"; draft: string }
  | { type: "flow/set_submitting"; isSubmitting: boolean }
  | { type: "flow/set_error"; error: string | null }
  | { type: "flow/set_mirrored_session"; sessionId: string | null }
  | { type: "flow/patch_setup"; patch: Partial<SetupState> }
  | { type: "flow/back_requested" };

const INITIAL_SETUP: SetupState = {
  mode: null,
  providerId: null,
  assistantName: "",
  actorId: null,
  actorDisplayName: null,
  channel: null,
  portId: null,
  portName: null,
  telegramHandle: null,
};

const INITIAL_STATE: OnboardState = {
  messages: [],
  step: "chooseMode",
  draft: "",
  isSubmitting: false,
  inlineError: null,
  mirroredSessionId: null,
  setup: INITIAL_SETUP,
};

const onboardReducer = createEventReducer<OnboardState, OnboardEvent>({
  "chat/append": (state, event) => ({
    state: { ...state, messages: [...state.messages, event.message] },
  }),
  "chat/append_many": (state, event) => ({
    state: { ...state, messages: [...state.messages, ...event.messages] },
  }),
  "chat/patch": (state, event) => ({
    state: {
      ...state,
      messages: state.messages.map((message) =>
        message.id === event.id
          ? {
              ...message,
              ...event.patch,
              timestamp:
                event.patch.timestamp ?? message.timestamp ?? nowTimestamp(),
            }
          : message
      ),
    },
  }),
  "chat/remove": (state, event) => ({
    state: {
      ...state,
      messages: state.messages.filter((message) => message.id !== event.id),
    },
  }),
  "flow/set_step": (state, event) => ({
    state: { ...state, step: event.step },
  }),
  "flow/set_draft": (state, event) => ({
    state: { ...state, draft: event.draft },
  }),
  "flow/set_submitting": (state, event) => ({
    state: { ...state, isSubmitting: event.isSubmitting },
  }),
  "flow/set_error": (state, event) => ({
    state: { ...state, inlineError: event.error },
  }),
  "flow/set_mirrored_session": (state, event) => ({
    state: { ...state, mirroredSessionId: event.sessionId },
  }),
  "flow/patch_setup": (state, event) => ({
    state: { ...state, setup: { ...state.setup, ...event.patch } },
  }),
  "flow/back_requested": (state) => {
    const previousStep: Step =
      state.step === "enterApiKey"
        ? "chooseMode"
        : state.step === "channelToken"
          ? "chooseChannel"
          : state.step === "testMessage"
            ? "channelToken"
            : state.step;
    return {
      state: {
        ...state,
        step: previousStep,
        inlineError: null,
        draft: "",
      },
    };
  },
});

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

function resolveRuntimeBaseUrl(): string {
  if (typeof window === "undefined") return "";
  const fromEnv =
    (import.meta as unknown as { env?: Record<string, string | undefined> }).env
      ?.VITE_BORG_API_BASE_URL ?? "";
  if (fromEnv.trim()) return fromEnv.replace(/\/+$/, "");

  const { protocol, hostname, port, origin } = window.location;
  const isLocal = hostname === "localhost" || hostname === "127.0.0.1";
  if (isLocal && (port === "5173" || port === "4173")) {
    return `${protocol}//${hostname}:8080`;
  }
  return origin;
}

type ChatActorPayload = {
  actorId: string;
  sessionId: string;
  userId: string;
  text: string;
  metadata?: Record<string, unknown>;
};

type ChatActorResponse = {
  session_id: string;
  reply?: string | null;
};

async function chatActor(payload: ChatActorPayload): Promise<ChatActorResponse> {
  const response = await fetch(`${resolveRuntimeBaseUrl()}/ports/http`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
    },
    body: JSON.stringify({
      user_key: payload.userId,
      session_id: payload.sessionId,
      actor_id: payload.actorId,
      text: payload.text,
      metadata: payload.metadata,
    }),
  });
  if (!response.ok) {
    throw new Error(`chat actor request failed (${response.status})`);
  }
  return (await response.json()) as ChatActorResponse;
}

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
  message: OnboardingSessionMessage
): "assistant" | "user" | "system" {
  const roleCandidate =
    (message.role?.trim().toLowerCase() ?? "") ||
    message.messageType.trim().toLowerCase();
  if (roleCandidate === "assistant") return "assistant";
  if (roleCandidate === "user") return "user";
  return "system";
}

function formatTimestamp(value: unknown): string {
  if (typeof value !== "string" || !value.trim()) return nowTimestamp();
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return nowTimestamp();
  return date.toLocaleTimeString();
}

function toMirroredChatMessages(
  rawMessages: OnboardingSessionMessage[]
): MirroredChatMessage[] {
  return [...rawMessages]
    .sort((left, right) => left.messageIndex - right.messageIndex)
    .map((message) => {
      const role = detectMessageRole(message);
      const text = message.text?.trim() ?? "";
      return {
        messageIdentity: message.id.trim()
          ? message.id
          : `${message.sessionId}:${message.messageIndex}`,
        role,
        text,
        timestamp: formatTimestamp(message.createdAt),
      };
    })
    .filter(
      (item) =>
        (item.role === "assistant" || item.role === "user") &&
        item.text.length > 0
    );
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
  const { state, dispatch, getState } = useStateReducer({
    initialState: INITIAL_STATE,
    reducer: onboardReducer,
  });
  const {
    messages,
    step,
    draft,
    isSubmitting,
    inlineError,
    mirroredSessionId,
    setup,
  } = state;
  const submitInFlightRef = React.useRef(false);
  const pollInFlightRef = React.useRef(false);
  const mirroredSessionRef = React.useRef<string | null>(null);
  const mirroredSeenRef = React.useRef<Set<string>>(new Set());

  const appendMessage = React.useCallback(
    (message: Omit<ChatMessageItem, "id" | "timestamp">) => {
      const next: ChatMessageItem = {
        id: `onboard-${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`,
        timestamp: nowTimestamp(),
        ...message,
      };
      dispatch({ type: "chat/append", message: next });
      return next.id;
    },
    [dispatch]
  );

  React.useEffect(() => {
    const timeoutId = window.setTimeout(() => {
      const current = getState();
      if (current.messages.some((message) => message.id === "onboard-intro")) {
        return;
      }
      dispatch({
        type: "chat/append",
        message: {
          id: "onboard-intro",
          role: "assistant",
          text: i18n.t("onboard.assistant.intro_single"),
          timestamp: nowTimestamp(),
        },
      });
    }, 520);
    return () => window.clearTimeout(timeoutId);
  }, [dispatch, getState]);

  const patchMessage = React.useCallback(
    (id: string, patch: Partial<ChatMessageItem>) => {
      dispatch({ type: "chat/patch", id, patch });
    },
    [dispatch]
  );

  const bootstrapOnboardingActor = React.useCallback(
    async (mode: AiMode, providerId: string | null) => {
      const actorId = createLocalId("borg:actor:onboard");
      const sessionId = createLocalId("borg:session:onboard");
      const userId = createLocalId("borg:user:onboard");

      await upsertOnboardingActor({
        actorId,
        name: "Onboarding Assistant",
        systemPrompt: ONBOARDING_ACTOR_PROMPT,
        status: "RUNNING",
      });

      dispatch({ type: "flow/set_step", step: "chooseChannel" });
      dispatch({ type: "flow/set_error", error: null });
      dispatch({ type: "flow/set_draft", draft: "" });

      // initial onboarding turn
      const pendingId = appendMessage({
        role: "assistant",
        text: "...",
        pending: true,
      });
      try {
        const response = await chatActor({
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
    [appendMessage, dispatch, patchMessage]
  );

  const handleModeSelection = React.useCallback(
    async (mode: AiMode) => {
      if (isSubmitting || step !== "chooseMode") return;
      dispatch({ type: "flow/set_error", error: null });
      dispatch({ type: "flow/patch_setup", patch: { mode } });
      appendMessage({ role: "user", text: modeLabel(mode) });

      if (mode === "local") {
        dispatch({ type: "flow/set_submitting", isSubmitting: true });
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
          dispatch({ type: "flow/set_step", step: "chooseMode" });
        } finally {
          dispatch({ type: "flow/set_submitting", isSubmitting: false });
        }
        return;
      }

      appendMessage({
        role: "assistant",
        text: i18n.t("onboard.assistant.ask_api_key", {
          provider: modeLabel(mode),
        }),
      });
      dispatch({ type: "flow/set_step", step: "enterApiKey" });
      dispatch({ type: "flow/set_draft", draft: "" });
    },
    [appendMessage, bootstrapOnboardingActor, dispatch, isSubmitting, step]
  );

  const handleApiKeySubmit = React.useCallback(
    async (submitted?: string) => {
      const apiKey = (submitted ?? draft).trim();
      if (!apiKey) {
        dispatch({
          type: "flow/set_error",
          error: i18n.t("onboard.error.api_key_required"),
        });
        return;
      }
      if (setup.mode !== "openai" && setup.mode !== "openrouter") {
        dispatch({
          type: "flow/set_error",
          error: i18n.t("onboard.error.mode_missing"),
        });
        return;
      }

      dispatch({ type: "flow/set_error", error: null });
      dispatch({ type: "flow/set_submitting", isSubmitting: true });
      appendMessage({ role: "user", text: maskSecret(apiKey) });
      const pendingId = appendMessage({
        role: "assistant",
        text: i18n.t("onboard.assistant.checking_credentials"),
        pending: true,
      });

      const providerId = createLocalId("borg:provider");
      try {
        await upsertProvider({
          provider: providerId,
          providerKind: setup.mode,
          apiKey,
          enabled: true,
        });
        await listProviderModels(providerId);
        dispatch({ type: "flow/patch_setup", patch: { providerId } });
        patchMessage(pendingId, {
          text: i18n.t("onboard.assistant.provider_connected", {
            provider: modeLabel(setup.mode as "openai" | "openrouter"),
          }),
          pending: false,
        });
        await bootstrapOnboardingActor(setup.mode, providerId);
      } catch {
        await deleteProvider(providerId, { ignoreNotFound: true });
        patchMessage(pendingId, {
          text: i18n.t("onboard.error.invalid_api_key"),
          pending: false,
          role: "system",
        });
        dispatch({
          type: "flow/set_error",
          error: i18n.t("onboard.error.invalid_api_key_retry"),
        });
      } finally {
        dispatch({ type: "flow/set_submitting", isSubmitting: false });
        dispatch({ type: "flow/set_draft", draft: "" });
      }
    },
    [
      appendMessage,
      bootstrapOnboardingActor,
      dispatch,
      draft,
      patchMessage,
      setup.mode,
    ]
  );

  const handleChannelSelection = React.useCallback(
    async (channel: Channel) => {
      if (isSubmitting || step !== "chooseChannel") return;
      dispatch({ type: "flow/set_error", error: null });
      dispatch({ type: "flow/set_submitting", isSubmitting: true });
      dispatch({ type: "flow/patch_setup", patch: { channel } });
      appendMessage({ role: "user", text: channelLabel(channel) });

      dispatch({ type: "flow/set_step", step: "channelToken" });
      dispatch({ type: "flow/set_draft", draft: "" });
      appendMessage({
        role: "assistant",
        text:
          channel === "telegram"
            ? i18n.t("onboard.assistant.ask_telegram_token")
            : i18n.t("onboard.assistant.ask_discord_token"),
      });

      dispatch({ type: "flow/set_submitting", isSubmitting: false });
    },
    [appendMessage, dispatch, isSubmitting, step]
  );

  const handleChannelTokenSubmit = React.useCallback(
    async (submitted?: string) => {
      const token = (submitted ?? draft).trim();
      const { channel } = setup;
      if (!channel) {
        dispatch({
          type: "flow/set_error",
          error: i18n.t("onboard.error.channel_missing"),
        });
        return;
      }
      if (!token) {
        dispatch({
          type: "flow/set_error",
          error:
            channel === "telegram"
              ? i18n.t("onboard.error.telegram_token_required")
              : i18n.t("onboard.error.discord_token_required"),
        });
        return;
      }

      if (
        channel === "telegram" &&
        !/^\d{6,}:[A-Za-z0-9_-]{20,}$/.test(token)
      ) {
        dispatch({
          type: "flow/set_error",
          error: i18n.t("onboard.error.telegram_token_invalid"),
        });
        appendMessage({
          role: "assistant",
          text: i18n.t("onboard.assistant.telegram_help"),
        });
        return;
      }
      if (channel === "discord" && token.length < 20) {
        dispatch({
          type: "flow/set_error",
          error: i18n.t("onboard.error.discord_token_invalid"),
        });
        return;
      }

      dispatch({ type: "flow/set_error", error: null });
      dispatch({ type: "flow/set_submitting", isSubmitting: true });
      appendMessage({ role: "user", text: maskSecret(token) });

      const botInfo =
        channel === "telegram"
          ? await fetchTelegramBotInfo(token)
          : { handle: null, displayName: null };
      const assistantName =
        botInfo.displayName || setup.assistantName || "Assistant";
      const slug = toSlug(assistantName) || "assistant";
      const actorId = setup.actorId ?? `borg:actor:${slug}-01`;
      const actorDisplayName = setup.actorDisplayName ?? `${assistantName}-01`;
      const unique = `${Date.now()}`.slice(-6);
      const portName = `${slug}-${channel}-${unique}`;
      const portId = `borg:port:${portName}`;

      try {
        if (!setup.actorId) {
          await upsertOnboardingActor({
            actorId,
            name: actorDisplayName,
            systemPrompt: DEFAULT_ASSISTANT_PROMPT,
            status: "RUNNING",
          });
        }

        const settings: Record<string, unknown> = { bot_token: token };
        if (channel === "telegram" || channel === "discord") {
          settings.allowed_external_user_ids = [];
        }

        await upsertOnboardingPort(portId, {
          provider: channel,
          enabled: true,
          allows_guests: true,
          assigned_actor_id: actorId,
          settings,
        });

        const telegramHandle = channel === "telegram" ? botInfo.handle : null;

        dispatch({
          type: "flow/patch_setup",
          patch: {
            assistantName,
            actorId,
            actorDisplayName,
            portId,
            portName,
            telegramHandle,
          },
        });

        dispatch({ type: "flow/set_step", step: "testMessage" });
        dispatch({ type: "flow/set_draft", draft: "" });
        dispatch({ type: "flow/set_mirrored_session", sessionId: null });
        mirroredSessionRef.current = null;
        mirroredSeenRef.current = new Set();
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
        dispatch({
          type: "flow/set_error",
          error:
            channel === "telegram"
              ? i18n.t("onboard.error.telegram_connect_retry")
              : i18n.t("onboard.error.discord_connect_retry"),
        });
      } finally {
        dispatch({ type: "flow/set_submitting", isSubmitting: false });
      }
    },
    [appendMessage, dispatch, draft, setup]
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
          const bindings = await listOnboardingPortBindingsByPortId(portId, 50);
          const match =
            bindings.find((binding) => binding.actorId === actorId) ??
            bindings[0] ??
            null;
          sessionId = match?.sessionId ?? null;
          if (sessionId) {
            dispatch({ type: "flow/set_mirrored_session", sessionId });
          }
        }
        if (!sessionId) return;
        if (mirroredSessionRef.current !== sessionId) {
          mirroredSessionRef.current = sessionId;
          mirroredSeenRef.current = new Set();
        }

        const rawMessages = await listOnboardingSessionMessages(sessionId, 250);
        if (!active || rawMessages.length === 0) return;

        const mapped = toMirroredChatMessages(rawMessages);
        let sawAssistant = false;
        const seenMessages = mirroredSeenRef.current;
        const nextMessages: ChatMessageItem[] = [];

        for (const item of mapped) {
          if (seenMessages.has(item.messageIdentity)) {
            continue;
          }
          seenMessages.add(item.messageIdentity);
          if (item.role === "assistant") {
            sawAssistant = true;
          }
          nextMessages.push({
            id: `onboard-mirror-${item.messageIdentity}`,
            role: item.role,
            text: item.text,
            timestamp: item.timestamp,
          });
        }

        if (nextMessages.length > 0) {
          dispatch({ type: "chat/append_many", messages: nextMessages });
        }

        if (sawAssistant && step === "testMessage") {
          dispatch({ type: "flow/set_step", step: "complete" });
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
  }, [
    dispatch,
    mirroredSessionId,
    setup.actorId,
    setup.channel,
    setup.portId,
    step,
  ]);

  const submitDraft = React.useCallback(async () => {
    if (isSubmitting || submitInFlightRef.current) return;
    submitInFlightRef.current = true;
    try {
      const submitted = draft;
      dispatch({ type: "flow/set_draft", draft: "" });
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
  }, [
    dispatch,
    draft,
    handleApiKeySubmit,
    handleChannelTokenSubmit,
    isSubmitting,
    step,
  ]);

  const canGoBack = step !== "chooseMode" && step !== "complete";
  const handleBack = React.useCallback(() => {
    dispatch({ type: "flow/back_requested" });
  }, [dispatch]);
  const setDraftValue = React.useCallback(
    (value: string) => {
      dispatch({ type: "flow/set_draft", draft: value });
    },
    [dispatch]
  );

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
        onChange={setDraftValue}
        onSubmit={() => void submitDraft()}
        isRunning={isSubmitting}
        placeholder={placeholder}
      />
    );
  }, [
    draft,
    setDraftValue,
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
