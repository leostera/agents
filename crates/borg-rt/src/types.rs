use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;

pub(crate) type FfiResult = Result<Value>;
pub(crate) type FfiHandler = Arc<dyn Fn(Vec<Value>) -> FfiResult + Send + Sync>;
pub(crate) type FfiRegistry = Arc<HashMap<String, FfiHandler>>;
