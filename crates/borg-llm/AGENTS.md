# borg-llm

Multi-provider LLM abstraction layer with unified API for chat completions and audio transcription.

## Structure

```
src/
├── completion.rs    # Typed public API plus raw provider-neutral request/response types
├── transcription.rs  # AudioTranscriptionRequest, AudioTranscriptionResponse, AudioSource
├── runner.rs        # LlmRunner typed adapter over raw providers
├── tools.rs         # TypedToolSet<C>, ToolCall<C>, TypedTool trait, raw tool metadata
├── response.rs      # TypedResponse<R> and raw response-format schema
├── error.rs         # Structured errors (Error, LlmResult)
├── capability.rs    # Capability enum
├── model.rs         # Model struct
├── testing/         # Shared Docker-backed test helpers (enabled via `testing` feature)
└── provider/
    ├── mod.rs       # Object-safe raw provider trait
    ├── openai.rs    # OpenAI raw provider adapter
    ├── anthropic.rs # Anthropic raw provider adapter
    ├── openrouter.rs # OpenRouter raw provider adapter
    ├── lm_studio.rs # LM Studio raw provider adapter
    └── ollama.rs    # Ollama raw provider adapter
```

## Key Patterns

### ModelSelector (not optional)
- `ModelSelector::Any` - Use provider's default
- `ModelSelector::Provider(provider_type)` - Specific provider
- `ModelSelector::Specific { provider, model }` - Specific model, optional provider hint

### Provider Implementation
Each provider implements `LlmProvider` trait with:
- `provider_type()` - Returns `ProviderType`
- `chat_raw(req: RawCompletionRequest)` - Returns `LlmResult<RawCompletionResponse>`
- `transcribe(req: AudioTranscriptionRequest)` - Returns `LlmResult<AudioTranscriptionResponse>`
- `available_models()` - Returns `LlmResult<Vec<Model>>`

Providers do not deserialize user-defined tool/response Rust types. They only translate between raw provider-neutral types and provider wire formats.

### Response Access Pattern
OpenAI-compatible APIs return `ChatResponse` with `choices[].message.content`, not `message.content` directly:
```rust
chat_res.choices[0].message.content.clone()
```

### AudioSource Serialization
Custom `Serialize`/`Deserialize` implementation using untagged enums for `Vec<u8>`, `String`, and `PathBuf` variants.

### Typed Tools and Responses
`CompletionRequest<C, R>` and `CompletionResponse<C, R>` are generic:
- `C` = tool call type (user's enum implementing `TypedTool`)
- `R` = response type (default `String`, or user's typed response struct)

`LlmRunner` itself is not generic. Type parameters live on each `chat` call.

```rust
// Default - string responses
CompletionResponse<MyTools, String>
→ message.content: String

// Typed responses  
CompletionResponse<MyTools, MyResponse>
→ message.content: MyResponse
```

Define tools by implementing the `TypedTool` trait:
```rust
impl TypedTool for MyTools {
    fn tool_definitions() -> Vec<RawToolDefinition> { ... }
    fn decode_tool_call(name: &str, arguments: serde_json::Value) -> LlmResult<Self> { ... }
}
```

## Commands

```bash
cargo build -p borg-llm
cargo test -p borg-llm
cargo clippy -p borg-llm
```

Shared test helpers live under `src/testing/` and real end-to-end cases live under `tests/`.
Use `cargo test -p borg-llm --features testing` to compile/run the Docker-backed e2e target.
Use `BORG_LLM_TEST_OLLAMA_MODEL=<model>` to override the Ollama model they pull.

## Adding a New Provider

1. Create `src/provider/<name>.rs`
2. Define provider-specific request/response types (derive `Deserialize`, `Serialize`, `Builder`)
3. Implement `LlmProvider` trait
4. Add to provider module exports in `mod.rs`
5. Handle `ModelSelector` pattern matching:
   ```rust
   let model = match req.model {
       ModelSelector::Any => self.config.default_model.clone(),
       ModelSelector::Provider(_) => self.config.default_model.clone(),
       ModelSelector::Specific { model, .. } => model,
   };
   ```
6. Normalize provider-specific tool calls into `RawToolCall { id, name, arguments }`
7. Return plain assistant text/JSON payload in `RawCompletionResponse.message.content`
