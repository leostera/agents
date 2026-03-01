use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct ShellModeContext {
    pub working_directory: Option<PathBuf>,
    pub timeout_seconds: Option<u64>,
}

impl ShellModeContext {
    pub fn with_working_directory(mut self, dir: PathBuf) -> Self {
        self.working_directory = Some(dir);
        self
    }

    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = Some(seconds);
        self
    }

    pub fn timeout(&self, default: Duration) -> Duration {
        self.timeout_seconds
            .map(Duration::from_secs)
            .unwrap_or(default)
    }

    pub fn working_directory(&self) -> Option<&PathBuf> {
        self.working_directory.as_ref()
    }
}
