use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ExecutionTarget {
    pub label: String,
    pub provider: String,
    pub model: String,
    pub max_in_flight: usize,
}

impl ExecutionTarget {
    pub fn new(
        label: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let provider = provider.into();
        Self {
            label: label.into(),
            max_in_flight: default_max_in_flight(&provider),
            provider,
            model: model.into(),
        }
    }

    pub fn ollama(label: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new(label, "ollama", model)
    }

    pub fn openai(label: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new(label, "openai", model)
    }

    pub fn anthropic(label: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new(label, "anthropic", model)
    }

    pub fn openrouter(label: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new(label, "openrouter", model)
    }

    pub fn with_max_in_flight(mut self, max_in_flight: usize) -> Self {
        self.max_in_flight = max_in_flight.max(1);
        self
    }

    pub fn display_label(&self) -> String {
        match self.provider.as_str() {
            "ollama" | "default" => self.label.clone(),
            provider => format!("{provider}:{}", self.label),
        }
    }
}

impl Default for ExecutionTarget {
    fn default() -> Self {
        Self::new("default", "default", "default")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunConfig {
    pub targets: Vec<ExecutionTarget>,
    pub trials: usize,
}

impl RunConfig {
    pub fn new(targets: Vec<ExecutionTarget>) -> Self {
        Self { targets, trials: 1 }
    }

    pub fn single(target: ExecutionTarget) -> Self {
        Self::new(vec![target])
    }

    pub fn with_trials(mut self, trials: usize) -> Self {
        self.trials = trials.max(1);
        self
    }
}

fn default_max_in_flight(provider: &str) -> usize {
    match provider {
        "ollama" => 1,
        _ => 5,
    }
}

impl Default for RunConfig {
    fn default() -> Self {
        Self::single(ExecutionTarget::default())
    }
}
