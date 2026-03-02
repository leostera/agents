const DEFAULT_OPENAI_DEVICE_CODE_URL: &str = "https://auth.openai.com/oauth/device/code";
const DEFAULT_OPENAI_DEVICE_CODE_SCOPE: &str = "openid profile email offline_access";
const DEFAULT_GITHUB_OAUTH_CLIENT_ID: &str = "Ov23lixRdrrvUNKtXcPO";

#[derive(Debug, Clone)]
pub struct Config {
    pub openai_oauth_client_id: Option<String>,
    pub openai_device_code_url: String,
    pub openai_device_code_scope: String,
    pub github_oauth_client_id: String,
    pub github_oauth_client_secret: Option<String>,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            openai_oauth_client_id: env_var_optional("BORG_OPENAI_OAUTH_CLIENT_ID"),
            openai_device_code_url: env_var_with_default(
                "BORG_OPENAI_DEVICE_CODE_URL",
                DEFAULT_OPENAI_DEVICE_CODE_URL,
            ),
            openai_device_code_scope: env_var_with_default(
                "BORG_OPENAI_DEVICE_CODE_SCOPE",
                DEFAULT_OPENAI_DEVICE_CODE_SCOPE,
            ),
            github_oauth_client_id: env_var_with_default(
                "BORG_GITHUB_OAUTH_CLIENT_ID",
                DEFAULT_GITHUB_OAUTH_CLIENT_ID,
            ),
            github_oauth_client_secret: env_var_optional("BORG_GITHUB_OAUTH_CLIENT_SECRET"),
        }
    }
}

fn env_var_optional(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn env_var_with_default(key: &str, default: &str) -> String {
    env_var_optional(key).unwrap_or_else(|| default.to_string())
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn defaults_include_openai_device_code_url_and_scope() {
        let config = Config::default();
        assert!(!config.openai_device_code_url.is_empty());
        assert!(!config.openai_device_code_scope.is_empty());
        assert!(!config.github_oauth_client_id.is_empty());
    }
}
