use async_trait::async_trait;
use std::ffi::{CStr, CString, c_void};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tempfile::NamedTempFile;

use crate::capability::Capability;
use crate::completion::{ModelSelector, ProviderType, RawCompletionRequest, RawCompletionResponse};
use crate::error::{Error, LlmResult};
use crate::model::Model;
use crate::provider::LlmProvider;
use crate::transcription::{
    AudioSource, AudioTranscriptionRequest, AudioTranscriptionResponse, TranscriptionFormat,
    TranscriptionLanguage, TranscriptionPrompt,
};

const APPLE_TRANSCRIPTION_MODEL: &str = "apple-speech";

#[derive(Debug, Clone, Default)]
pub struct AppleConfig {
    pub default_locale: Option<String>,
    pub default_model: String,
}

impl AppleConfig {
    pub fn new() -> Self {
        Self {
            default_locale: None,
            default_model: APPLE_TRANSCRIPTION_MODEL.to_string(),
        }
    }

    pub fn with_default_locale(mut self, locale: impl Into<String>) -> Self {
        self.default_locale = Some(locale.into());
        self
    }
}

pub struct Apple {
    config: AppleConfig,
}

impl Apple {
    pub fn new(config: AppleConfig) -> Self {
        Self { config }
    }

    fn resolve_model(&self, model: &ModelSelector) -> LlmResult<String> {
        match model {
            ModelSelector::Any | ModelSelector::Provider(_) => {
                Ok(self.config.default_model.clone())
            }
            ModelSelector::Specific { model, .. } if model == &self.config.default_model => {
                Ok(model.clone())
            }
            ModelSelector::Specific { model, .. } => Err(Error::InvalidRequest {
                reason: format!(
                    "Apple transcription provider only supports model {} (got {model})",
                    self.config.default_model
                ),
            }),
        }
    }

    fn resolve_locale(&self, language: &TranscriptionLanguage) -> Option<String> {
        match language {
            TranscriptionLanguage::AutoDetect => self.config.default_locale.clone(),
            TranscriptionLanguage::Explicit { language } => Some(language.clone()),
        }
    }

    fn validate_transcription_request(&self, req: &AudioTranscriptionRequest) -> LlmResult<()> {
        if !matches!(req.prompt, TranscriptionPrompt::None) {
            return Err(Error::InvalidRequest {
                reason: "Apple transcription does not support prompt hints".to_string(),
            });
        }

        if !matches!(
            req.response_format,
            TranscriptionFormat::ProviderDefault | TranscriptionFormat::Text
        ) {
            return Err(Error::InvalidRequest {
                reason: "Apple transcription only supports text output".to_string(),
            });
        }

        Ok(())
    }

    fn prepare_audio_path(&self, audio: &AudioSource) -> LlmResult<PreparedAudio> {
        match audio {
            AudioSource::Path(path) => Ok(PreparedAudio::Existing(path.clone())),
            AudioSource::Url(_) => Err(Error::InvalidRequest {
                reason: "Apple transcription does not support URL audio".to_string(),
            }),
            AudioSource::Data(data) => {
                let extension = infer_audio_extension(data);
                let mut temp =
                    NamedTempFile::with_suffix(format!(".{extension}")).map_err(|error| {
                        Error::Internal {
                            message: format!("failed to create temporary audio file: {error}"),
                        }
                    })?;

                std::io::Write::write_all(&mut temp, data).map_err(|error| Error::Internal {
                    message: format!("failed to write temporary audio file: {error}"),
                })?;

                Ok(PreparedAudio::Temp(temp))
            }
        }
    }
}

enum PreparedAudio {
    Existing(PathBuf),
    Temp(NamedTempFile),
}

impl PreparedAudio {
    fn path(&self) -> &Path {
        match self {
            PreparedAudio::Existing(path) => path.as_path(),
            PreparedAudio::Temp(file) => file.path(),
        }
    }
}

#[cfg(target_os = "macos")]
#[link(name = "Speech", kind = "framework")]
unsafe extern "C" {}
#[cfg(target_os = "macos")]
#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn borg_apple_transcribe_file(
        path: *const c_char,
        locale: *const c_char,
        callback: *const c_void,
    ) -> i32;
}

#[cfg(target_os = "macos")]
enum CallbackPayload {
    Transcript(String),
    Debug(String),
}

#[cfg(target_os = "macos")]
static APPLE_CALLBACKS: OnceLock<Mutex<Vec<CallbackPayload>>> = OnceLock::new();

#[cfg(target_os = "macos")]
extern "C" fn apple_transcription_callback(text: *const c_char, status: i32) {
    if text.is_null() {
        return;
    }

    let message = unsafe { CStr::from_ptr(text) }
        .to_string_lossy()
        .to_string();

    if message.trim().is_empty() {
        return;
    }

    let payload = if status < 0 {
        CallbackPayload::Debug(message)
    } else {
        CallbackPayload::Transcript(message)
    };

    if let Some(lock) = APPLE_CALLBACKS.get()
        && let Ok(mut callbacks) = lock.lock()
    {
        callbacks.push(payload);
    }
}

#[cfg(target_os = "macos")]
fn take_callback_messages() -> Vec<CallbackPayload> {
    let lock = APPLE_CALLBACKS.get_or_init(|| Mutex::new(Vec::new()));
    let mut callbacks = lock.lock().expect("apple callback lock poisoned");
    let mut drained = Vec::new();
    std::mem::swap(&mut *callbacks, &mut drained);
    drained
}

fn infer_audio_extension(data: &[u8]) -> &'static str {
    if data.starts_with(b"OggS") {
        return "ogg";
    }
    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WAVE") {
        return "wav";
    }
    if data.starts_with(b"fLaC") {
        return "flac";
    }
    if data.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return "webm";
    }
    if let Some(kind) = infer::get(data) {
        match kind.mime_type() {
            "audio/ogg" => "ogg",
            "audio/mpeg" => "mp3",
            "audio/x-wav" | "audio/wav" => "wav",
            "audio/flac" => "flac",
            "video/webm" | "audio/webm" => "webm",
            _ => "wav",
        }
    } else {
        "wav"
    }
}

fn map_start_error(code: i32) -> Error {
    match code {
        1 => Error::Provider {
            provider: "apple".to_string(),
            status: 503,
            message: "speech recognizer unavailable".to_string(),
        },
        2 => Error::Authentication {
            provider: "apple".to_string(),
            message: "speech recognition authorization denied".to_string(),
        },
        4 => Error::InvalidResponse {
            reason: "apple transcription returned no transcript".to_string(),
        },
        5 => Error::InvalidRequest {
            reason: "invalid apple transcription arguments".to_string(),
        },
        6 => Error::Provider {
            provider: "apple".to_string(),
            status: 504,
            message: "apple transcription timed out".to_string(),
        },
        _ => Error::Internal {
            message: format!("apple transcription failed with code {code}"),
        },
    }
}

#[async_trait]
impl LlmProvider for Apple {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Apple
    }

    fn provider_name(&self) -> &'static str {
        "apple"
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::AudioTranscription]
    }

    async fn available_models(&self) -> LlmResult<Vec<Model>> {
        Ok(vec![Model::new(self.config.default_model.clone())])
    }

    async fn chat_raw(&self, _req: RawCompletionRequest) -> LlmResult<RawCompletionResponse> {
        Err(Error::InvalidRequest {
            reason: "Apple provider only supports audio transcription".to_string(),
        })
    }

    async fn transcribe(
        &self,
        req: AudioTranscriptionRequest,
    ) -> LlmResult<AudioTranscriptionResponse> {
        self.validate_transcription_request(&req)?;
        let model = self.resolve_model(&req.model)?;

        #[cfg(not(target_os = "macos"))]
        {
            let _ = model;
            let _ = req;
            return Err(Error::InvalidRequest {
                reason: "Apple transcription is only available on macOS".to_string(),
            });
        }

        #[cfg(target_os = "macos")]
        {
            let prepared = self.prepare_audio_path(&req.audio)?;
            let path = prepared.path();
            if !path.exists() {
                return Err(Error::InvalidRequest {
                    reason: format!("audio path does not exist: {}", path.display()),
                });
            }

            let locale = self.resolve_locale(&req.language);
            let c_path =
                CString::new(path.as_os_str().to_string_lossy().as_bytes()).map_err(|_| {
                    Error::InvalidRequest {
                        reason: "audio path contains interior null bytes".to_string(),
                    }
                })?;
            let c_locale = locale
                .as_ref()
                .map(|value| CString::new(value.as_str()))
                .transpose()
                .map_err(|_| Error::InvalidRequest {
                    reason: "locale contains interior null bytes".to_string(),
                })?;

            let _ = take_callback_messages();
            let code = unsafe {
                borg_apple_transcribe_file(
                    c_path.as_ptr(),
                    c_locale
                        .as_ref()
                        .map_or(std::ptr::null(), |locale| locale.as_ptr()),
                    apple_transcription_callback as *const () as *const c_void,
                )
            };

            let messages = take_callback_messages();
            if code != 0 {
                let debug = messages
                    .iter()
                    .filter_map(|message| match message {
                        CallbackPayload::Debug(text) => Some(text.as_str()),
                        CallbackPayload::Transcript(_) => None,
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                let mut error = map_start_error(code);
                if !debug.is_empty() {
                    error = match error {
                        Error::Provider {
                            provider,
                            status,
                            message,
                        } => Error::Provider {
                            provider,
                            status,
                            message: format!("{message}; {debug}"),
                        },
                        Error::Authentication { provider, message } => Error::Authentication {
                            provider,
                            message: format!("{message}; {debug}"),
                        },
                        Error::InvalidResponse { reason } => Error::InvalidResponse {
                            reason: format!("{reason}; {debug}"),
                        },
                        other => other,
                    };
                }
                return Err(error);
            }

            let transcript = messages
                .iter()
                .filter_map(|message| match message {
                    CallbackPayload::Transcript(text) if !text.trim().is_empty() => {
                        Some(text.as_str())
                    }
                    CallbackPayload::Transcript(_) | CallbackPayload::Debug(_) => None,
                })
                .next_back()
                .map(str::to_string)
                .ok_or_else(|| Error::InvalidResponse {
                    reason: "apple transcription returned no transcript".to_string(),
                })?;

            Ok(AudioTranscriptionResponse {
                provider: ProviderType::Apple,
                model,
                text: transcript,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        APPLE_TRANSCRIPTION_MODEL, Apple, AppleConfig, infer_audio_extension, map_start_error,
    };
    use crate::completion::{
        ModelSelector, ProviderType, RawCompletionRequest, ResponseMode, Temperature, TokenLimit,
        ToolChoice, TopK, TopP,
    };
    use crate::error::Error;
    use crate::provider::LlmProvider;
    use crate::transcription::{AudioSource, AudioTranscriptionRequest, TranscriptionFormat};

    #[tokio::test]
    async fn apple_provider_only_reports_transcription_capability() {
        let provider = Apple::new(AppleConfig::new());
        assert_eq!(provider.provider_type(), ProviderType::Apple);
        assert_eq!(provider.provider_name(), "apple");
        assert_eq!(provider.capabilities().len(), 1);
        assert!(provider.capabilities()[0].supports_transcription());
    }

    #[test]
    fn infer_audio_extension_detects_ogg() {
        let audio = include_bytes!("../../tests/fixtures/1-2-3-hello-world.ogg");
        assert_eq!(infer_audio_extension(audio), "ogg");
    }

    #[tokio::test]
    async fn apple_provider_rejects_prompt_hints() {
        let provider = Apple::new(AppleConfig::new());
        let error = provider
            .transcribe(
                AudioTranscriptionRequest::new(AudioSource::Data(vec![1, 2, 3]))
                    .with_prompt("bias words"),
            )
            .await
            .expect_err("prompt hints should be rejected");

        assert!(matches!(error, Error::InvalidRequest { reason } if reason.contains("prompt")));
    }

    #[tokio::test]
    async fn apple_provider_rejects_non_text_response_formats() {
        let provider = Apple::new(AppleConfig::new());
        let error = provider
            .transcribe(
                AudioTranscriptionRequest::new(AudioSource::Data(vec![1, 2, 3]))
                    .with_response_format(TranscriptionFormat::VerboseJson),
            )
            .await
            .expect_err("non-text response formats should be rejected");

        assert!(
            matches!(error, Error::InvalidRequest { reason } if reason.contains("text output"))
        );
    }

    #[tokio::test]
    async fn apple_provider_rejects_url_audio() {
        let provider = Apple::new(AppleConfig::new());
        let error = provider
            .transcribe(AudioTranscriptionRequest::new(AudioSource::Url(
                "https://example.com/audio.ogg".to_string(),
            )))
            .await
            .expect_err("URL audio should be rejected");

        assert!(matches!(error, Error::InvalidRequest { reason } if reason.contains("URL audio")));
    }

    #[tokio::test]
    async fn apple_provider_rejects_unknown_models() {
        let provider = Apple::new(AppleConfig::new());
        let error = provider
            .transcribe(
                AudioTranscriptionRequest::new(AudioSource::Data(vec![1, 2, 3]))
                    .with_model(ModelSelector::from_model("other-model")),
            )
            .await
            .expect_err("unknown model should be rejected");

        assert!(
            matches!(error, Error::InvalidRequest { reason } if reason.contains(APPLE_TRANSCRIPTION_MODEL))
        );
    }

    #[tokio::test]
    async fn apple_provider_rejects_chat_requests() {
        let provider = Apple::new(AppleConfig::new());
        let error = provider
            .chat_raw(RawCompletionRequest {
                model: ModelSelector::for_provider(ProviderType::Apple),
                input: vec![],
                temperature: Temperature::ProviderDefault,
                top_p: TopP::ProviderDefault,
                top_k: TopK::ProviderDefault,
                token_limit: TokenLimit::ProviderDefault,
                response_mode: ResponseMode::Buffered,
                tools: None,
                tool_choice: ToolChoice::ProviderDefault,
                response_format: None,
            })
            .await
            .expect_err("chat should be rejected");

        assert!(
            matches!(error, Error::InvalidRequest { reason } if reason.contains("audio transcription"))
        );
    }

    #[test]
    fn apple_start_errors_are_typed() {
        let timeout = map_start_error(6);
        let denied = map_start_error(2);

        assert!(matches!(timeout, Error::Provider { status: 504, .. }));
        assert!(matches!(denied, Error::Authentication { .. }));
    }
}
