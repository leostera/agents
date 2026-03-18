#[derive(Debug, Clone)]
pub enum Capability {
    ChatCompletion,
    Completion,
    AudioTranscription,
    Evals,
}

impl Capability {
    pub fn supports_transcription(&self) -> bool {
        matches!(self, Capability::AudioTranscription)
    }

    pub fn supports_chat(&self) -> bool {
        matches!(self, Capability::ChatCompletion)
    }

    pub fn supports_completion(&self) -> bool {
        matches!(self, Capability::Completion)
    }

    pub fn supports_evals(&self) -> bool {
        matches!(self, Capability::Evals)
    }
}
