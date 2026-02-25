use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Uri;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub entity_id: Uri,
    pub entity_type: Uri,
    pub label: String,
    pub props: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
