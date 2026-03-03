use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::Uri;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EntityPropValue {
    Text(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Bytes(Vec<u8>),
    Ref(Uri),
    List(Vec<EntityPropValue>),
}

impl EntityPropValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[EntityPropValue]> {
        match self {
            Self::List(values) => Some(values),
            _ => None,
        }
    }
}

pub type EntityProps = BTreeMap<String, EntityPropValue>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub entity_id: Uri,
    pub entity_type: Uri,
    pub label: String,
    pub props: EntityProps,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
