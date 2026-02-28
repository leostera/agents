const DEFAULT_OPENAI_DEVICE_CODE_URL: &str = "https://auth.openai.com/oauth/device/code";
const DEFAULT_OPENAI_DEVICE_CODE_SCOPE: &str = "openid profile email offline_access";

#[derive(Debug, Clone)]
pub struct Config {
    pub openai_oauth_client_id: Option<String>,
    pub openai_device_code_url: String,
    pub openai_device_code_scope: String,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            openai_oauth_client_id: option_env!("BORG_OPENAI_OAUTH_CLIENT_ID")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
            openai_device_code_url: option_env!("BORG_OPENAI_DEVICE_CODE_URL")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_OPENAI_DEVICE_CODE_URL)
                .to_string(),
            openai_device_code_scope: option_env!("BORG_OPENAI_DEVICE_CODE_SCOPE")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_OPENAI_DEVICE_CODE_SCOPE)
                .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn defaults_include_openai_device_code_url_and_scope() {
        let config = Config::default();
        assert!(!config.openai_device_code_url.is_empty());
        assert!(!config.openai_device_code_scope.is_empty());
    }
}
