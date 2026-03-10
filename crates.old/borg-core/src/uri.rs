use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// A Borg URI. Format: `scheme:[//]path`
///
/// This implementation is intentionally flexible to support both
/// `borg:actor:id` and `tg://chat/id` as specified in RFD0033.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Uri(String);

impl Uri {
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        let input = input.trim();
        if input.is_empty() {
            anyhow::bail!("URI cannot be empty");
        }

        // Basic validation: must have a scheme and a colon
        if !input.contains(':') {
            anyhow::bail!("invalid URI: `{}` (missing scheme)", input);
        }

        Ok(Self(input.to_string()))
    }

    pub fn from_parts(ns: &str, kind: &str, id: Option<&str>) -> anyhow::Result<Self> {
        let value = match id {
            Some(id) => format!("{}:{}:{}", ns, kind, id),
            None => format!("{}:{}", ns, kind),
        };
        Self::parse(&value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Uri {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<str> for Uri {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
