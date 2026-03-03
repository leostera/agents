export type Locale = "en";

export type MessageKey =
  | "onboard.assistant.welcome"
  | "onboard.assistant.choose_provider"
  | "onboard.provider.openai"
  | "onboard.provider.openrouter"
  | "onboard.title"
  | "onboard.tagline"
  | "onboard.assistant.provider_key_prompt"
  | "onboard.choice.provider_name"
  | "onboard.choice.provider_placeholder"
  | "onboard.user_turn"
  | "onboard.field.api_key"
  | "onboard.action.save_api_key"
  | "onboard.action.saving"
  | "onboard.notice.saved"
  | "onboard.error.save_failed"
  | "onboard.choice.default_placeholder"
  | "dashboard.title"
  | "dashboard.tagline"
  | "dashboard.step"
  | "dashboard.subtitle.default"
  | "dashboard.subtitle.settings.providers"
  | "web.unknown_route";

type Vars = Record<string, string | number>;

const messages: Record<Locale, Record<MessageKey, string>> = {
  en: {
    "onboard.assistant.welcome":
      "Hello {username}! Welcome to Borg.\n\nBefore we proceed, we need to set up an LLM provider, please choose one from the following list:",
    "onboard.title": "Borg Onboarding",
    "onboard.tagline": "A guided chat setup for your first Borg run.",
    "onboard.assistant.choose_provider": "Choose an LLM provider.",
    "onboard.provider.openai": "OpenAI API Key",
    "onboard.provider.openrouter": "OpenRouter API Key",
    "onboard.assistant.provider_key_prompt":
      "Great choice. Please paste your {provider} below and click Save. It will be stored in ~/.borg/config.db under providers.",
    "onboard.choice.provider_name": "provider",
    "onboard.choice.provider_placeholder": "Select an LLM provider",
    "onboard.user_turn": "YOUR TURN",
    "onboard.field.api_key": "API Key",
    "onboard.action.save_api_key": "Save API Key",
    "onboard.action.saving": "Saving...",
    "onboard.notice.saved": "Saved. You can now run borg start.",
    "onboard.error.save_failed": "Failed to save {provider} API key",
    "onboard.choice.default_placeholder": "Select an option",
    "dashboard.title": "Borg Dashboard",
    "dashboard.tagline": "Dashboard UI package is ready for the next step.",
    "dashboard.step": "Dashboard",
    "dashboard.subtitle.default": "Platform and session intelligence",
    "dashboard.subtitle.settings.providers":
      "Configure AI Providers, limits, models, and fallback policies",
    "web.unknown_route": "Unknown route",
  },
};

export function createI18n(locale: Locale = "en") {
  return {
    t(key: MessageKey, vars?: Vars): string {
      const template = messages[locale][key];
      if (!vars) return template;
      return Object.entries(vars).reduce(
        (acc, [name, value]) => acc.replaceAll(`{${name}}`, String(value)),
        template
      );
    },
  };
}
