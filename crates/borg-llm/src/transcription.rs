use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::completion::ProviderType;

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
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub model: Option<crate::completion::ModelSelector>,
    pub response_format: Option<String>,
}

impl AudioTranscriptionRequest {
    pub fn builder() -> AudioTranscriptionRequestBuilder {
        AudioTranscriptionRequestBuilder::default()
    }
}

#[derive(Debug, Clone, Builder, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTranscriptionRequestBuilder {
    pub audio: Option<AudioSource>,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub model: Option<crate::completion::ModelSelector>,
    pub response_format: Option<String>,
}

impl Default for AudioTranscriptionRequestBuilder {
    fn default() -> Self {
        Self {
            audio: None,
            language: None,
            prompt: None,
            model: None,
            response_format: None,
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
