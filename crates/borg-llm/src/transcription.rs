use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::completion::{ModelSelector, ProviderType};

#[derive(Debug, Clone)]
pub enum AudioSource {
    Data(Vec<u8>),
    Url(String),
    Path(PathBuf),
}

impl Serialize for AudioSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            AudioSource::Data(data) => {
                #[derive(Serialize)]
                struct DataVariant {
                    data: Vec<u8>,
                }
                DataVariant { data: data.clone() }.serialize(serializer)
            }
            AudioSource::Url(url) => {
                #[derive(Serialize)]
                struct UrlVariant {
                    url: String,
                }
                UrlVariant { url: url.clone() }.serialize(serializer)
            }
            AudioSource::Path(path) => {
                #[derive(Serialize)]
                struct PathVariant {
                    path: String,
                }
                PathVariant {
                    path: path.to_string_lossy().to_string(),
                }
                .serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for AudioSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RawAudioSource {
            Data { data: Vec<u8> },
            Url { url: String },
            Path { path: String },
        }

        let raw = RawAudioSource::deserialize(deserializer)?;
        match raw {
            RawAudioSource::Data { data } => Ok(AudioSource::Data(data)),
            RawAudioSource::Url { url } => Ok(AudioSource::Url(url)),
            RawAudioSource::Path { path } => Ok(AudioSource::Path(PathBuf::from(path))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTranscriptionRequest {
    pub audio: AudioSource,
    pub language: TranscriptionLanguage,
    pub prompt: TranscriptionPrompt,
    pub model: ModelSelector,
    pub response_format: TranscriptionFormat,
}

impl AudioTranscriptionRequest {
    pub fn new(audio: AudioSource) -> Self {
        Self {
            audio,
            language: TranscriptionLanguage::AutoDetect,
            prompt: TranscriptionPrompt::None,
            model: ModelSelector::Any,
            response_format: TranscriptionFormat::ProviderDefault,
        }
    }

    pub fn builder() -> AudioTranscriptionRequestBuilder {
        AudioTranscriptionRequestBuilder::default()
    }

    pub fn with_model(mut self, model: ModelSelector) -> Self {
        self.model = model;
        self
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = TranscriptionLanguage::explicit(language);
        self
    }

    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = TranscriptionPrompt::hint(prompt);
        self
    }

    pub fn with_response_format(mut self, response_format: TranscriptionFormat) -> Self {
        self.response_format = response_format;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioTranscriptionRequestBuilder {
    pub audio: Option<AudioSource>,
    pub language: TranscriptionLanguage,
    pub prompt: TranscriptionPrompt,
    pub model: ModelSelector,
    pub response_format: TranscriptionFormat,
}

impl Default for AudioTranscriptionRequestBuilder {
    fn default() -> Self {
        Self {
            audio: None,
            language: TranscriptionLanguage::AutoDetect,
            prompt: TranscriptionPrompt::None,
            model: ModelSelector::Any,
            response_format: TranscriptionFormat::ProviderDefault,
        }
    }
}

impl AudioTranscriptionRequestBuilder {
    pub fn with_audio(mut self, audio: AudioSource) -> Self {
        self.audio = Some(audio);
        self
    }

    pub fn with_model(mut self, model: ModelSelector) -> Self {
        self.model = model;
        self
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = TranscriptionLanguage::explicit(language);
        self
    }

    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = TranscriptionPrompt::hint(prompt);
        self
    }

    pub fn with_response_format(mut self, response_format: TranscriptionFormat) -> Self {
        self.response_format = response_format;
        self
    }

    pub fn build(self) -> crate::error::LlmResult<AudioTranscriptionRequest> {
        let audio = self
            .audio
            .ok_or_else(|| crate::error::Error::InvalidRequest {
                reason: "Audio transcription request requires an audio source".to_string(),
            })?;

        Ok(AudioTranscriptionRequest {
            audio,
            language: self.language,
            prompt: self.prompt,
            model: self.model,
            response_format: self.response_format,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum TranscriptionLanguage {
    AutoDetect,
    Explicit { language: String },
}

impl TranscriptionLanguage {
    pub fn explicit(language: impl Into<String>) -> Self {
        Self::Explicit {
            language: language.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum TranscriptionPrompt {
    None,
    Hint { text: String },
}

impl TranscriptionPrompt {
    pub fn hint(text: impl Into<String>) -> Self {
        Self::Hint { text: text.into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionFormat {
    ProviderDefault,
    Text,
    Json,
    VerboseJson,
    DiarizedJson,
    Srt,
    Vtt,
}

impl TranscriptionFormat {
    pub fn as_openai_str(&self) -> Option<&'static str> {
        match self {
            TranscriptionFormat::ProviderDefault => None,
            TranscriptionFormat::Text => Some("text"),
            TranscriptionFormat::Json => Some("json"),
            TranscriptionFormat::VerboseJson => Some("verbose_json"),
            TranscriptionFormat::DiarizedJson => Some("diarized_json"),
            TranscriptionFormat::Srt => Some("srt"),
            TranscriptionFormat::Vtt => Some("vtt"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTranscriptionResponse {
    pub provider: ProviderType,
    pub model: String,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::{
        AudioSource, AudioTranscriptionRequest, TranscriptionFormat, TranscriptionLanguage,
        TranscriptionPrompt,
    };
    use crate::completion::ModelSelector;
    use crate::error::Error;

    #[test]
    fn transcription_request_defaults_capture_intent() {
        let request = AudioTranscriptionRequest::new(AudioSource::Data(vec![1, 2, 3]));

        assert!(matches!(request.model, ModelSelector::Any));
        assert!(matches!(
            request.language,
            TranscriptionLanguage::AutoDetect
        ));
        assert!(matches!(request.prompt, TranscriptionPrompt::None));
        assert_eq!(
            request.response_format,
            TranscriptionFormat::ProviderDefault
        );
    }

    #[test]
    fn transcription_request_builder_requires_audio() {
        let error = AudioTranscriptionRequest::builder()
            .build()
            .expect_err("missing audio should fail");

        assert!(matches!(error, Error::InvalidRequest { .. }));
    }
}
