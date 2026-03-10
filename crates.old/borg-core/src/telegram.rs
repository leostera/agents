use crate::Uri;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TelegramUserId {
    Numeric(String),
    Username(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramUserIdParseError {
    input: String,
}

impl fmt::Display for TelegramUserIdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid telegram user id `{}` (expected numeric id like 2654566 or username like @leostera)",
            self.input
        )
    }
}

impl std::error::Error for TelegramUserIdParseError {}

impl TelegramUserId {
    pub fn from_sender_id(sender_id: u64) -> Self {
        Self::Numeric(sender_id.to_string())
    }

    pub fn from_sender_username(username: &str) -> Option<Self> {
        let with_prefix = if username.trim().starts_with('@') {
            username.trim().to_string()
        } else {
            format!("@{}", username.trim())
        };
        let normalized = normalize_username(&with_prefix)?;
        Some(Self::Username(normalized))
    }

    pub fn to_uri(&self) -> Uri {
        // Invariant: validated Telegram user ids are always valid URI tail parts.
        Uri::from_parts("telegram", "user", Some(&self.to_string()))
            .expect("telegram user id must map to a valid URI")
    }

    pub fn into_uri(self) -> Uri {
        self.to_uri()
    }
}

impl fmt::Display for TelegramUserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TelegramUserId::Numeric(value) | TelegramUserId::Username(value) => {
                write!(f, "{value}")
            }
        }
    }
}

impl FromStr for TelegramUserId {
    type Err = TelegramUserIdParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let raw = value.trim();
        if raw.is_empty() {
            return Err(TelegramUserIdParseError {
                input: value.to_string(),
            });
        }

        if let Some(rest) = raw.strip_prefix("telegram:user:")
            && !rest.is_empty()
            && rest.chars().all(|c| c.is_ascii_digit())
        {
            return Ok(Self::Numeric(rest.to_string()));
        }

        if raw.chars().all(|c| c.is_ascii_digit()) {
            return Ok(Self::Numeric(raw.to_string()));
        }

        if let Some(normalized) = normalize_username(raw) {
            return Ok(Self::Username(normalized));
        }

        Err(TelegramUserIdParseError {
            input: value.to_string(),
        })
    }
}

fn normalize_username(value: &str) -> Option<String> {
    let raw = value.trim();
    let username = raw.strip_prefix('@')?;
    let len = username.chars().count();
    if !(5..=32).contains(&len) {
        return None;
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    Some(format!("@{}", username.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::TelegramUserId;
    use std::str::FromStr;

    #[test]
    fn parses_numeric_id() {
        let parsed = TelegramUserId::from_str("2654566").unwrap();
        assert_eq!(parsed.to_string(), "2654566");
    }

    #[test]
    fn parses_username_and_normalizes_case() {
        let parsed = TelegramUserId::from_str("@LeoStera").unwrap();
        assert_eq!(parsed.to_string(), "@leostera");
    }

    #[test]
    fn parses_sender_username_without_at_prefix() {
        let parsed = TelegramUserId::from_sender_username("LeoStera").unwrap();
        assert_eq!(parsed.to_string(), "@leostera");
    }

    #[test]
    fn parses_legacy_telegram_user_prefix() {
        let parsed = TelegramUserId::from_str("telegram:user:2654566").unwrap();
        assert_eq!(parsed.to_string(), "2654566");
    }

    #[test]
    fn rejects_invalid_username() {
        assert!(TelegramUserId::from_str("@bad").is_err());
        assert!(TelegramUserId::from_str("bad-user").is_err());
    }

    #[test]
    fn converts_numeric_to_uri() {
        let parsed = TelegramUserId::from_str("2654566").unwrap();
        assert_eq!(parsed.into_uri().to_string(), "telegram:user:2654566");
    }

    #[test]
    fn converts_username_to_uri() {
        let parsed = TelegramUserId::from_str("@LeoStera").unwrap();
        assert_eq!(parsed.into_uri().to_string(), "telegram:user:@leostera");
    }
}
