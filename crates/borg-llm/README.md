# borg-llm

`borg-llm` contains the model-facing layer for the workspace.

It provides:

- `LlmRunner`
- provider integrations like Ollama, OpenAI, Anthropic, OpenRouter, and LM Studio
- typed completions and tool-call decoding
- shared completion and transcription types

## Example

```rust
use borg_llm::{InputItem, LlmRunner};

fn message() -> InputItem {
    InputItem::from("hello world")
}

fn build_runner() -> LlmRunner {
    LlmRunner::builder().build()
}
```
