use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MacOsPolicy {
    pub allowed_shortcuts: BTreeSet<String>,
    pub allowed_script_templates: BTreeSet<String>,
    pub allow_raw_applescript: bool,
    pub max_execution_seconds: u64,
}

impl MacOsPolicy {
    pub fn is_shortcut_allowed(&self, shortcut_name: &str) -> bool {
        if self.allowed_shortcuts.is_empty() {
            return true;
        }
        self.allowed_shortcuts.contains(shortcut_name)
    }

    pub fn is_template_allowed(&self, template_id: &str) -> bool {
        if self.allowed_script_templates.is_empty() {
            return true;
        }
        self.allowed_script_templates.contains(template_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacOsExecutionData {
    pub command: String,
    pub exit_code: i32,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}
