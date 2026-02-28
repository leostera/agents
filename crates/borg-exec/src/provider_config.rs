#[derive(Clone, Debug, Default)]
pub struct ProviderConfigSnapshot {
    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
    pub openai_api_mode: Option<String>,
    pub openrouter_api_key: Option<String>,
    pub openrouter_base_url: Option<String>,
    pub preferred_provider: Option<String>,
}
