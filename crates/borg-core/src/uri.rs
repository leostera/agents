use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Uri(Url);

impl Uri {
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        let url = Url::parse(input)?;
        Ok(Self(url))
    }

    pub fn from_parts(ns: &str, kind: &str, id: Option<&str>) -> anyhow::Result<Self> {
        let value = match id {
            Some(id) => format!("{}:{}:{}", ns, kind, id),
            None => format!("{}:{}", ns, kind),
        };
        Self::parse(&value)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Uri {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for Uri {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Uri {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Uri::parse(&raw).map_err(serde::de::Error::custom)
    }
}
