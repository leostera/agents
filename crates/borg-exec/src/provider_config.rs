#[derive(Clone, Debug, Default)]
pub struct ProviderConfigSnapshot {
    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
    pub openai_api_mode: Option<String>,
    pub openai_default_text_model: Option<String>,
    pub openai_default_audio_model: Option<String>,
    pub openrouter_api_key: Option<String>,
    pub openrouter_base_url: Option<String>,
    pub openrouter_default_text_model: Option<String>,
    pub openrouter_default_audio_model: Option<String>,
    pub preferred_provider: Option<String>,
}
