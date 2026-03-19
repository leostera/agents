use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::llm::capability::Capability;
use crate::llm::completion::{
    FinishReason, ModelSelector, ProviderType, RawCompletionRequest, RawCompletionResponse,
    RawInputContent, RawInputItem, RawOutputContent, RawOutputItem, Role,
    ToolChoice as RawToolChoice, Usage as CompletionUsage,
};
use crate::llm::error::{Error, LlmResult, WorkersAIConfigError};
use crate::llm::model::Model;
use crate::llm::provider::LlmProvider;
use crate::llm::response::RawResponseFormat;
use crate::llm::tools::{RawToolCall, RawToolDefinition};
use crate::llm::transcription::{AudioTranscriptionRequest, AudioTranscriptionResponse};

#[derive(Debug, Clone)]
pub struct WorkersAIConfig {
    pub api_token: String,
    pub account_id: String,
    pub base_url: String,
    pub default_model: String,
}

impl WorkersAIConfig {
    pub fn new(
        api_token: impl Into<String>,
        account_id: impl Into<String>,
        default_model: impl Into<String>,
    ) -> Result<Self, WorkersAIConfigError> {
        let api_token = api_token.into();
        if api_token.is_empty() {
            return Err(WorkersAIConfigError::MissingApiToken);
        }

        let account_id = account_id.into();
        if account_id.is_empty() {
            return Err(WorkersAIConfigError::MissingAccountId);
        }

        Ok(Self {
            api_token,
            base_url: default_base_url(&account_id),
            account_id,
            default_model: default_model.into(),
        })
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = normalize_base_url(base_url.into());
        self
    }
}

pub struct WorkersAI {
    client: Client,
    config: WorkersAIConfig,
}

impl WorkersAI {
    pub fn new(config: WorkersAIConfig) -> Self {
        Self {
            client: Client::builder()
                .build()
                .expect("failed to build reqwest client"),
            config,
        }
    }
}

#[async_trait]
impl LlmProvider for WorkersAI {
    fn provider_type(&self) -> ProviderType {
        ProviderType::WorkersAI
    }

    fn provider_name(&self) -> &'static str {
        "workers_ai"
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::ChatCompletion]
    }

    async fn available_models(&self) -> LlmResult<Vec<Model>> {
        Ok(vec![Model::new(self.config.default_model.clone())])
    }

    async fn chat_raw(&self, req: RawCompletionRequest) -> LlmResult<RawCompletionResponse> {
        let model = match &req.model {
            ModelSelector::Any => self.config.default_model.clone(),
            ModelSelector::Provider(_) => self.config.default_model.clone(),
            ModelSelector::Specific { model, .. } => model.clone(),
        };

        if model.is_empty() {
            return Err(Error::NoMatchingProvider {
                reason: "Workers AI requires a model to be specified".to_string(),
            });
        }

        let url = format!("{}/run/{model}", self.config.base_url);
        let body = build_run_request(req);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(Error::Provider {
                provider: "workers_ai".to_string(),
                status: status.as_u16(),
                message: body,
            });
        }

        let envelope: RunResponseEnvelope =
            serde_json::from_str(&body).map_err(|error| Error::parse(&body, error))?;
        if !envelope.success {
            return Err(Error::Provider {
                provider: "workers_ai".to_string(),
                status: status.as_u16(),
                message: body,
            });
        }
        let result = envelope.result;
        let tool_calls = result
            .tool_calls
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(parse_tool_call)
            .collect::<LlmResult<Vec<_>>>()?;
        let output = raw_output_from_result(&result, tool_calls);
        let finish_reason = if output
            .iter()
            .any(|item| matches!(item, RawOutputItem::ToolCall { .. }))
        {
            FinishReason::ToolCalls
        } else {
            FinishReason::Stop
        };

        Ok(RawCompletionResponse {
            provider: ProviderType::WorkersAI,
            model,
            output,
            usage: CompletionUsage {
                prompt_tokens: result.usage.as_ref().map_or(0, |usage| usage.prompt_tokens),
                completion_tokens: result
                    .usage
                    .as_ref()
                    .map_or(0, |usage| usage.completion_tokens),
                total_tokens: result.usage.as_ref().map_or(0, |usage| usage.total_tokens),
            },
            finish_reason,
        })
    }

    async fn transcribe(
        &self,
        _req: AudioTranscriptionRequest,
    ) -> LlmResult<AudioTranscriptionResponse> {
        Err(Error::InvalidRequest {
            reason: "Workers AI transcription is not supported by this provider yet".to_string(),
        })
    }
}

fn default_base_url(account_id: &str) -> String {
    format!("https://api.cloudflare.com/client/v4/accounts/{account_id}/ai")
}

fn normalize_base_url(base_url: String) -> String {
    let base_url = base_url.trim_end_matches('/').to_string();
    if let Some(stripped) = base_url.strip_suffix("/v1") {
        stripped.to_string()
    } else {
        base_url
    }
}

fn flatten_content(content: &[RawInputContent]) -> String {
    content
        .iter()
        .filter_map(|content| match content {
            RawInputContent::Text { text } => Some(text.as_str()),
            RawInputContent::ImageUrl { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_tool_call(call: RunToolCall) -> LlmResult<RawToolCall> {
    Ok(RawToolCall {
        id: call.id,
        name: call.name,
        arguments: call.arguments,
    })
}

fn raw_output_from_result(result: &RunResult, tool_calls: Vec<RawToolCall>) -> Vec<RawOutputItem> {
    let mut output = Vec::new();
    if let Some(value) = &result.response {
        match value {
            Value::String(content) if !content.is_empty() => {
                output.push(RawOutputItem::Message {
                    role: Role::Assistant,
                    content: vec![RawOutputContent::Text {
                        text: content.clone(),
                    }],
                });
            }
            Value::Null => {}
            value => {
                output.push(RawOutputItem::Message {
                    role: Role::Assistant,
                    content: vec![RawOutputContent::Json {
                        value: value.clone(),
                    }],
                });
            }
        }
    } else if let Some(content) = result
        .result
        .as_deref()
        .filter(|content| !content.is_empty())
    {
        output.push(RawOutputItem::Message {
            role: Role::Assistant,
            content: vec![RawOutputContent::Text {
                text: content.to_string(),
            }],
        });
    }
    output.extend(
        tool_calls
            .into_iter()
            .map(|call| RawOutputItem::ToolCall { call }),
    );
    output
}

fn build_run_request(req: RawCompletionRequest) -> Value {
    let mut body = serde_json::Map::new();
    body.insert(
        "messages".to_string(),
        Value::Array(
            req.input
                .iter()
                .filter_map(|item| match item {
                    RawInputItem::Message { role, content } => Some(json!({
                        "role": match role {
                            Role::System => "system",
                            Role::User => "user",
                            Role::Assistant => "assistant",
                        },
                        "content": flatten_content(content),
                    })),
                    RawInputItem::ToolCall { .. } | RawInputItem::ToolResult { .. } => None,
                })
                .collect(),
        ),
    );

    if let Some(temperature) = req.temperature.as_option() {
        body.insert("temperature".to_string(), json!(temperature));
    }
    if let Some(top_p) = req.top_p.as_option() {
        body.insert("top_p".to_string(), json!(top_p));
    }
    if let Some(top_k) = req.top_k.as_option_i32() {
        body.insert("top_k".to_string(), json!(top_k));
    }
    if let Some(max_tokens) = req.token_limit.as_option() {
        body.insert("max_tokens".to_string(), json!(max_tokens));
    }
    if req.response_mode.is_streaming() {
        body.insert("stream".to_string(), Value::Bool(true));
    }
    if let Some(response_format) = req.response_format {
        body.insert(
            "response_format".to_string(),
            map_response_format(response_format),
        );
    }
    if let Some(tools) = req.tools {
        body.insert("tools".to_string(), map_tool_definitions(tools));
    }
    match req.tool_choice {
        RawToolChoice::ProviderDefault | RawToolChoice::Auto => {}
        RawToolChoice::Required => {
            body.insert("tool_choice".to_string(), json!("required"));
        }
        RawToolChoice::Specific { name } => {
            body.insert("tool_choice".to_string(), json!({ "name": name }));
        }
        RawToolChoice::None => {
            body.insert("tool_choice".to_string(), json!("none"));
        }
    }

    Value::Object(body)
}

fn map_tool_definitions(tools: Vec<RawToolDefinition>) -> Value {
    Value::Array(
        tools
            .into_iter()
            .map(|tool| {
                json!({
                    "name": tool.function.name,
                    "description": tool.function.description,
                    "parameters": tool.function.parameters,
                })
            })
            .collect(),
    )
}

fn map_response_format(format: RawResponseFormat) -> Value {
    match format.json_schema {
        Some(schema) => json!({
            "type": format.r#type,
            "json_schema": {
                "name": schema.name,
                "strict": schema.strict,
                "schema": schema.schema,
            }
        }),
        None => json!({ "type": format.r#type }),
    }
}

#[derive(Debug, Deserialize)]
struct RunResponseEnvelope {
    success: bool,
    result: RunResult,
}

#[derive(Debug, Deserialize)]
struct RunResult {
    #[serde(default)]
    response: Option<Value>,
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<RunToolCall>>,
    #[serde(default)]
    usage: Option<RunUsage>,
}

#[derive(Debug, Clone, Deserialize)]
struct RunToolCall {
    #[serde(default = "default_tool_call_id")]
    id: String,
    name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Debug, Deserialize)]
struct RunUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    total_tokens: u32,
}

fn default_tool_call_id() -> String {
    "workers_ai_tool_call".to_string()
}

#[cfg(test)]
mod tests {
    use super::{WorkersAI, WorkersAIConfig, default_base_url, normalize_base_url};
    use crate::llm::completion::ProviderType;
    use crate::llm::provider::LlmProvider;

    #[test]
    fn workers_ai_config_builds_default_base_url() {
        let config = WorkersAIConfig::new("token", "account", "@cf/meta/llama-3.1-8b-instruct")
            .expect("config");
        assert_eq!(config.base_url, default_base_url("account"));
    }

    #[test]
    fn workers_ai_config_normalizes_openai_compatible_base_urls() {
        let config = WorkersAIConfig::new("token", "account", "@cf/meta/llama-3.1-8b-instruct")
            .expect("config")
            .with_base_url("https://api.cloudflare.com/client/v4/accounts/account/ai/v1");
        assert_eq!(
            config.base_url,
            "https://api.cloudflare.com/client/v4/accounts/account/ai"
        );
        assert_eq!(
            normalize_base_url("https://api.cloudflare.com/client/v4/accounts/account/ai/".into()),
            "https://api.cloudflare.com/client/v4/accounts/account/ai"
        );
    }

    #[test]
    fn workers_ai_reports_provider_identity() {
        let provider = WorkersAI::new(
            WorkersAIConfig::new("token", "account", "@cf/meta/llama-3.1-8b-instruct")
                .expect("config"),
        );
        assert_eq!(provider.provider_type(), ProviderType::WorkersAI);
        assert_eq!(provider.provider_name(), "workers_ai");
    }
}
