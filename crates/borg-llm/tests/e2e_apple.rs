#![cfg(target_os = "macos")]

use std::path::PathBuf;

use borg_llm::error::LlmResult;
use borg_llm::provider::LlmProvider;
use borg_llm::provider::apple::{Apple, AppleConfig};
use borg_llm::runner::LlmRunner;
use borg_llm::transcription::{AudioSource, AudioTranscriptionRequest};
use serial_test::serial;

fn apple_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/1-2-3-hello-world.ogg")
}

fn assert_transcript_shape(text: &str) {
    let transcript = text.trim().to_lowercase();
    assert!(!transcript.is_empty(), "expected non-empty transcript");
    assert!(
        (transcript.contains("1") || transcript.contains("one"))
            && (transcript.contains("2") || transcript.contains("two"))
            && (transcript.contains("3") || transcript.contains("three"))
            && transcript.contains("hello")
            && transcript.contains("world"),
        "expected transcript to mention one/two/three/hello/world, got {transcript:?}",
    );
}

#[tokio::test]
#[serial]
async fn apple_provider_transcribes_audio_long() -> LlmResult<()> {
    let provider = Apple::new(AppleConfig::new());
    let response = provider
        .transcribe(
            AudioTranscriptionRequest::new(AudioSource::Path(apple_fixture_path()))
                .with_language("en-US"),
        )
        .await?;

    assert_transcript_shape(&response.text);
    Ok(())
}

#[tokio::test]
#[serial]
async fn apple_runner_transcribes_audio_long() -> LlmResult<()> {
    let runner = LlmRunner::builder()
        .add_provider(Apple::new(AppleConfig::new()))
        .build();

    let response = runner
        .transcribe(
            AudioTranscriptionRequest::new(AudioSource::Path(apple_fixture_path()))
                .with_language("en-US"),
        )
        .await?;

    assert_transcript_shape(&response.text);
    Ok(())
}
