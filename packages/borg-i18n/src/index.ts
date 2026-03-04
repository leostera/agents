export type Locale = "en";

export type MessageKey =
  | "onboard.title_v2"
  | "onboard.tagline_v2"
  | "onboard.assistant.intro_single"
  | "onboard.assistant.welcome_v2"
  | "onboard.assistant.moment_of_truth"
  | "onboard.assistant.choose_mode"
  | "onboard.mode.openai"
  | "onboard.mode.openrouter"
  | "onboard.mode.local"
  | "onboard.assistant.local_mode_selected"
  | "onboard.assistant.ask_api_key"
  | "onboard.error.api_key_required"
  | "onboard.error.mode_missing"
  | "onboard.assistant.checking_credentials"
  | "onboard.assistant.provider_connected"
  | "onboard.error.invalid_api_key"
  | "onboard.error.invalid_api_key_retry"
  | "onboard.assistant.ask_assistant_name"
  | "onboard.error.assistant_name_required"
  | "onboard.assistant.creating_assistant"
  | "onboard.assistant.assistant_ready"
  | "onboard.assistant.choose_channel"
  | "onboard.channel.telegram"
  | "onboard.channel.discord"
  | "onboard.channel.telegram_recommended"
  | "onboard.assistant.ask_telegram_token"
  | "onboard.assistant.ask_discord_token"
  | "onboard.assistant.telegram_ready"
  | "onboard.assistant.telegram_ready_no_link"
  | "onboard.error.channel_missing"
  | "onboard.error.telegram_token_required"
  | "onboard.error.discord_token_required"
  | "onboard.error.telegram_token_invalid"
  | "onboard.assistant.telegram_help"
  | "onboard.error.discord_token_invalid"
  | "onboard.assistant.connecting_channel"
  | "onboard.assistant.channel_connected"
  | "onboard.assistant.say_hi_prompt"
  | "onboard.error.telegram_connect_failed"
  | "onboard.error.discord_connect_failed"
  | "onboard.error.telegram_connect_retry"
  | "onboard.error.discord_connect_retry"
  | "onboard.error.bootstrap_failed"
  | "onboard.user.sent_hi"
  | "onboard.assistant.completion_intro"
  | "onboard.composer.choose_mode"
  | "onboard.composer.choose_channel"
  | "onboard.test.instructions"
  | "onboard.test.telegram_handle"
  | "onboard.action.confirm_hi_reply"
  | "onboard.action.try_again"
  | "onboard.assistant.retry_test_message"
  | "onboard.summary.provider_local"
  | "onboard.summary.provider_connected"
  | "onboard.complete.title"
  | "onboard.summary.assistant"
  | "onboard.summary.channel"
  | "onboard.summary.telegram_handle"
  | "onboard.complete.close_window"
  | "onboard.placeholder.api_key"
  | "onboard.placeholder.assistant_name"
  | "onboard.placeholder.channel_token"
  | "web.unknown_route";

type Vars = Record<string, string | number>;

const messages: Record<Locale, Record<MessageKey, string>> = {
  en: {
    "onboard.title_v2": "Borg Onboarding",
    "onboard.tagline_v2":
      "Set up your first assistant in chat, then send your first Telegram message.",
    "onboard.assistant.intro_single":
      "Hi! We are Borg. Let's get you set up!\n\nToday we'll connect to a provider, and get your Telegram bot connected to us.",
    "onboard.assistant.welcome_v2":
      "Welcome to Borg. This setup is designed for first-time users and takes just a few steps.",
    "onboard.assistant.moment_of_truth":
      "Goal: message your new Telegram bot and get a useful reply.",
    "onboard.assistant.choose_mode":
      "Choose how your assistant should run: connect a cloud AI provider or continue in local mode.",
    "onboard.mode.openai": "OpenAI",
    "onboard.mode.openrouter": "OpenRouter",
    "onboard.mode.local": "Run AI on this Computer",
    "onboard.assistant.local_mode_selected":
      "Local mode enabled. You can upgrade to OpenAI or OpenRouter later for stronger results.",
    "onboard.assistant.ask_api_key":
      "Paste your {provider} API key. We'll validate it immediately.",
    "onboard.error.api_key_required": "API key is required.",
    "onboard.error.mode_missing": "Choose an AI mode first.",
    "onboard.assistant.checking_credentials": "Checking credentials...",
    "onboard.assistant.provider_connected": "Connected to {provider}.",
    "onboard.error.invalid_api_key":
      "That key does not look valid. Please check it and try again.",
    "onboard.error.invalid_api_key_retry":
      "Try again with a valid key, or choose a different mode.",
    "onboard.assistant.ask_assistant_name":
      "Great. What should we call your assistant?",
    "onboard.error.assistant_name_required": "Assistant name is required.",
    "onboard.assistant.creating_assistant":
      "Creating your assistant with smart defaults...",
    "onboard.assistant.assistant_ready":
      "Done. {name} is ready and running in the background.",
    "onboard.assistant.choose_channel":
      "Now connect your first channel. Telegram is the fastest path.",
    "onboard.channel.telegram": "Telegram",
    "onboard.channel.discord": "Discord",
    "onboard.channel.telegram_recommended": "Telegram (Recommended)",
    "onboard.assistant.ask_telegram_token":
      "Next step is to get your BotToken from BotFather. Click [here](tg://resolve?domain=BotFather&text=%2Fnewbot) to get started!",
    "onboard.assistant.ask_discord_token":
      "Paste your Discord bot token. We'll set it up now.",
    "onboard.assistant.telegram_ready":
      "Brilliant! You can now say hi to {botName} on Telegram!\n\n[Start conversation]({link})",
    "onboard.assistant.telegram_ready_no_link":
      "Brilliant! You can now say hi to {botName} on Telegram!",
    "onboard.error.channel_missing": "Choose a channel first.",
    "onboard.error.telegram_token_required": "Telegram bot token is required.",
    "onboard.error.discord_token_required": "Discord bot token is required.",
    "onboard.error.telegram_token_invalid":
      "That Telegram token format looks wrong.",
    "onboard.assistant.telegram_help":
      "How to get a Telegram token:\n1. Open Telegram\n2. Message @BotFather\n3. Run /newbot\n4. Copy the token and paste it here\n\nTip: [Open BotFather and prefill /newbot](tg://resolve?domain=BotFather&text=%2Fnewbot)",
    "onboard.error.discord_token_invalid":
      "That Discord token format looks wrong.",
    "onboard.assistant.connecting_channel": "Connecting {channel}...",
    "onboard.assistant.channel_connected": "{channel} connected.",
    "onboard.assistant.say_hi_prompt":
      'Your bot is live on {channel}. Send "hi" now, then come back and confirm.',
    "onboard.error.telegram_connect_failed":
      "Could not connect Telegram with that token.",
    "onboard.error.discord_connect_failed":
      "Could not connect Discord with that token.",
    "onboard.error.telegram_connect_retry":
      "Please verify the token and try again. If needed, regenerate it in @BotFather.",
    "onboard.error.discord_connect_retry":
      "Please verify the token and try again.",
    "onboard.error.bootstrap_failed":
      "Unable to start onboarding chat. Please try again.",
    "onboard.user.sent_hi": "I sent hi and got a reply.",
    "onboard.assistant.completion_intro":
      "Excellent. Setup is complete and your assistant is running.",
    "onboard.composer.choose_mode": "Choose AI mode",
    "onboard.composer.choose_channel": "Choose channel",
    "onboard.test.instructions":
      'Send "hi" to your bot now. We\'ll mirror that conversation here automatically.',
    "onboard.test.telegram_handle": "Telegram handle: {handle}",
    "onboard.action.confirm_hi_reply": "I Got A Reply",
    "onboard.action.try_again": "Try Again",
    "onboard.assistant.retry_test_message":
      'No rush. Send "hi" when you\'re ready, then confirm.',
    "onboard.summary.provider_local": "Provider: Local mode",
    "onboard.summary.provider_connected": "Provider: {provider}",
    "onboard.complete.title": "You're Live",
    "onboard.summary.assistant": "Assistant: {name}",
    "onboard.summary.channel": "Channel: {channel}",
    "onboard.summary.telegram_handle": "Telegram: {handle}",
    "onboard.complete.close_window":
      "You can close this window. Your assistant keeps running.",
    "onboard.placeholder.api_key": "Paste API key...",
    "onboard.placeholder.assistant_name": "Name your assistant...",
    "onboard.placeholder.channel_token": "Paste bot token...",
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
