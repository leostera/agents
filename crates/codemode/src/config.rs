/// Configuration applied to a [`CodeMode`](crate::CodeMode) instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeModeConfig {
    multithreaded: bool,
    default_search_limit: usize,
}

impl Default for CodeModeConfig {
    fn default() -> Self {
        Self {
            multithreaded: false,
            default_search_limit: 20,
        }
    }
}

impl CodeModeConfig {
    /// Enables or disables multithreaded execution for the engine.
    pub fn multithreaded(mut self, value: bool) -> Self {
        self.multithreaded = value;
        self
    }

    /// Sets the default number of search matches returned when a request does
    /// not specify a limit.
    pub fn default_search_limit(mut self, value: usize) -> Self {
        self.default_search_limit = value.max(1);
        self
    }

    /// Returns whether multithreaded execution is enabled.
    pub fn is_multithreaded(&self) -> bool {
        self.multithreaded
    }

    /// Returns the default search limit.
    pub fn search_limit(&self) -> usize {
        self.default_search_limit
    }
}
