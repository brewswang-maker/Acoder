//! LLM Client — 统一的 LLM 调用接口
//!
//! 支持 OpenAI / DeepSeek / Qwen / GLM / MiniMax

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::{LlmConfig, LlmModel, LlmProvider};
use crate::error::{Error, Result};

pub use crate::llm::router::ModelRouter;


// ── 数据结构 ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// OpenAI requires tool_call_id for tool role messages ( Anthropic/minimax compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "lowercase")]
#[allow(missing_docs)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: MessageRole::System, content: content.into(), name: None, tool_call_id: None }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: MessageRole::User, content: content.into(), name: None, tool_call_id: None }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: MessageRole::Assistant, content: content.into(), name: None, tool_call_id: None }
    }
    pub fn tool(content: impl Into<String>, name: impl Into<String>, tool_call_id: impl Into<String>) -> Self {
        Self { role: MessageRole::Tool, content: content.into(), name: Some(name.into()), tool_call_id: Some(tool_call_id.into()) }
    }
}

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    pub stream: bool,
    pub tools: Option<Vec<LlmTool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmTool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub model: String,
    pub usage: TokenUsage,
    pub finish_reason: String,
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

// ── Trait 定义 ─────────────────────────────────────────────

#[async_trait]
pub trait LlmClientTrait: Send + Sync {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse>;
    async fn complete_streaming(&self, request: LlmRequest) -> Result<crate::llm::StreamingChunk>;
    fn model_info(&self, _model_id: &str) -> Option<LlmModel> { None }
}

#[derive(Debug, Clone)]
pub struct StreamingChunk {
    pub delta: String,
    pub done: bool,
    pub usage: Option<TokenUsage>,
}

// ── Client 实现 ─────────────────────────────────────────────

pub struct Client {
    config: Arc<LlmConfig>,
    cache: Arc<crate::llm::cache::ResponseCache>,
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            cache: self.cache.clone(),
        }
    }
}


#[async_trait]
impl LlmClientTrait for Client {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        Client::complete(self, request).await
    }
    async fn complete_streaming(&self, request: LlmRequest) -> Result<crate::llm::StreamingChunk> {
        Client::complete_streaming(self, request).await
    }
}

impl Client {
    pub fn new(config: LlmConfig) -> Self {
        Self {
            config: Arc::new(config),
            cache: Arc::new(crate::llm::cache::ResponseCache::new()),
        }
    }

    pub async fn for_provider(&self, provider_name: &str) -> Result<ProviderClient> {
        let provider = self.config.providers
            .get(provider_name)
            .ok_or_else(|| Error::ConfigInvalid {
                key: format!("llm.providers.{}", provider_name),
                reason: "Provider 不存在".into(),
            })?;

        let api_key = provider.api_key.as_ref()
            .ok_or(Error::LlmAuthFailed)?;

        Ok(match provider.api_type {
            crate::config::LlmApiType::OpenAi | crate::config::LlmApiType::OpenAiCompatible => {
                ProviderClient::OpenAi(OpenAiClient::new(provider, api_key))
            }
            crate::config::LlmApiType::Anthropic => {
                ProviderClient::Anthropic(AnthropicClient::new(provider, api_key))
            }
            crate::config::LlmApiType::Vertex | crate::config::LlmApiType::Bedrock => {
                ProviderClient::OpenAi(OpenAiClient::new(provider, api_key))
            }
        })
    }

    pub async fn complete(&self, mut request: LlmRequest) -> Result<LlmResponse> {
        // 缓存查询
        if let Some(cached) = self.cache.get(&request.model, &request.messages).await {
            tracing::debug!("LLM 缓存命中: {}", &request.model);
            return Ok(cached);
        }

        let model_id = if request.model == "auto" {
            self.config.default_provider.clone()
        } else {
            request.model.clone()
        };
        let provider_name = self.resolve_provider(&model_id);
        let provider = self.config.providers.get(&provider_name)
            .ok_or_else(|| Error::LlmModelUnavailable { model: model_id.clone() })?;

        if request.model == "auto" {
            request.model = provider.default_model.clone();
        }
        let model_id = request.model.clone();
        let client = self.for_provider(&provider_name).await?;
        let response = client.complete(request.clone()).await?;

        // 写入缓存
        self.cache.set(&request.model, &request.messages, &response).await;
        Ok(response)
    }

    fn resolve_provider(&self, model_id: &str) -> String {
        eprintln!("[DEBUG] resolve_provider called with model_id={}, default_provider={}", model_id, self.config.default_provider);
        let resolved = if model_id == "auto" {
            self.config.default_provider.clone()
        } else {
            model_id.to_string()
        };
        eprintln!("[DEBUG] resolved={}, providers={:?}", resolved, self.config.providers.keys().collect::<Vec<_>>());
        tracing::debug!("resolve_provider({}) -> resolved={}, default_provider={}, providers={:?}",
            model_id, resolved, self.config.default_provider,
            self.config.providers.keys().cloned().collect::<Vec<_>>());
        if self.config.providers.contains_key(&resolved) {
            resolved
        } else {
            tracing::warn!("unknown model {}, using default provider {}", resolved, self.config.default_provider);
            self.config.default_provider.clone()
        }
    }

    pub fn model_info(&self, model_id: &str) -> Option<LlmModel> {
        for provider in self.config.providers.values() {
            if let Some(m) = provider.models.iter().find(|m| m.id == model_id) {
                return Some(m.clone());
            }
        }
        None
    }

    pub fn config(&self) -> &LlmConfig {
        &self.config
    }
}

// ── Provider 级别 Client ─────────────────────────────────────

pub enum ProviderClient {
    OpenAi(OpenAiClient),
    Anthropic(AnthropicClient),
}

#[async_trait]
impl LlmClientTrait for ProviderClient {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        match self {
            ProviderClient::OpenAi(c) => c.complete(request).await,
            ProviderClient::Anthropic(c) => c.complete(request).await,
        }
    }

    async fn complete_streaming(&self, request: LlmRequest) -> Result<StreamingChunk> {
        match self {
            ProviderClient::OpenAi(c) => c.complete_streaming(request).await,
            ProviderClient::Anthropic(c) => c.complete_streaming(request).await,
        }
    }
}

// ── OpenAI Client ───────────────────────────────────────────

pub struct OpenAiClient {
    provider: Arc<LlmProvider>,
    http: reqwest::Client,
}

impl OpenAiClient {
    pub fn new(provider: &LlmProvider, _api_key: &str) -> Self {
        Self {
            provider: Arc::new(provider.clone()),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(provider.timeout_secs))
                .build()
                .unwrap_or_default(),
        }
    }

    pub async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let api_key = self.provider.api_key.as_ref().unwrap();

        let mut body_map = serde_json::Map::new();
        body_map.insert("model".into(), serde_json::json!(request.model));
        body_map.insert("messages".into(), serde_json::json!(request.messages));
        body_map.insert("temperature".into(), serde_json::json!(request.temperature.unwrap_or(0.7)));
        body_map.insert("max_tokens".into(), serde_json::json!(request.max_tokens.unwrap_or(4096)));
        body_map.insert("stream".into(), serde_json::json!(false));

        // 序列化 tools 字段（转换为 OpenAI format）
        if let Some(tools) = &request.tools {
            let openai_tools: Vec<serde_json::Value> = tools.iter().map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            }).collect();
            body_map.insert("tools".into(), serde_json::json!(openai_tools));
            // GLM 不支持带 tools 的请求中包含 temperature
            body_map.remove("temperature");
        }

        let body = serde_json::Value::Object(body_map.clone());
        let body_json = serde_json::to_string_pretty(&body).unwrap_or_default();
        let _ = std::fs::write("/tmp/llm_request_body.json", &body_json);
        eprintln!("[DEBUG] OpenAiClient body written to /tmp/llm_request_body.json ({} bytes)", body_json.len());

        let resp = self.http
            .post(format!("{}/chat/completions", self.provider.base_url))
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            tracing::error!("LLM API 错误: {} - {}", status, body_text);

            if status.as_u16() == 429 {
                return Err(Error::LlmRateLimited { retry_after: 60 });
            }
            return Err(Error::LlmFailed { reason: format!("HTTP {}: {}", status, body_text) });
        }

        let parsed: OpenAiResponse = resp.json().await
            .map_err(|e| Error::LlmFailed { reason: format!("响应解析失败: {}", e) })?;

        let choice = parsed.choices.first();
        let content = choice
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        let tool_calls = choice
            .and_then(|c| c.message.tool_calls.clone())
            .map(|tc| tc.into_iter().map(|tc| ToolCall {
                id: tc.id,
                name: tc.function.name,
                arguments: tc.function.arguments,
            }).collect());

        Ok(LlmResponse {
            content,
            model: request.model,
            usage: TokenUsage {
                input_tokens: parsed.usage.prompt_tokens,
                output_tokens: parsed.usage.completion_tokens,
                total_tokens: parsed.usage.total_tokens,
            },
            finish_reason: choice.map(|c| c.finish_reason.clone()).unwrap_or_default(),
            tool_calls,
        })
    }

    pub async fn complete_streaming(&self, request: LlmRequest) -> Result<StreamingChunk> {
        self.complete(request).await?;
        Ok(StreamingChunk { delta: String::new(), done: true, usage: None })
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    id: String,
    model: String,
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
    #[serde(rename = "finish_reason")]
    finish_reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenAiMessage {
    content: Option<String>,
    #[serde(rename = "tool_calls", default)]
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "function")]
    function: OpenAiFunction,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAiFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
}

// ── Anthropic Client ────────────────────────────────────────

pub struct AnthropicClient {
    provider: Arc<LlmProvider>,
    http: reqwest::Client,
}

impl AnthropicClient {
    pub fn new(provider: &LlmProvider, _api_key: &str) -> Self {
        Self {
            provider: Arc::new(provider.clone()),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(provider.timeout_secs))
                .build()
                .unwrap_or_default(),
        }
    }

    pub async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let api_key = self.provider.api_key.as_ref().unwrap();

        let system = request.messages.iter()
            .filter(|m| m.role == MessageRole::System)
            .map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        let messages: Vec<_> = request.messages.iter()
            .filter(|m| m.role != MessageRole::System)
            .map(|m| serde_json::json!({
                "role": m.role.to_string(),
                "content": m.content,
            }))
            .collect();

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "stream": false,
        });

        if !system.is_empty() {
            body["system"] = serde_json::Value::String(system);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::Value::Number(serde_json::Number::from_f64(temp as f64).unwrap_or_else(|| serde_json::Number::from(7)));
        }

        let resp = self.http
            .post(format!("{}/messages", self.provider.base_url))
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            tracing::error!("Anthropic API 错误: {} - {}", status, body_text);

            if status.as_u16() == 429 {
                return Err(Error::LlmRateLimited { retry_after: 60 });
            }
            return Err(Error::LlmFailed { reason: format!("HTTP {}: {}", status, body_text) });
        }

        let parsed: AnthropicResponse = resp.json().await
            .map_err(|e| Error::LlmFailed { reason: format!("响应解析失败: {}", e) })?;

        let content = parsed.content.first()
            .and_then(|c| c.get("text").and_then(|t| t.as_str()).map(String::from))
            .unwrap_or_default();

        Ok(LlmResponse {
            content,
            model: request.model,
            usage: TokenUsage {
                input_tokens: parsed.usage.input_tokens,
                output_tokens: parsed.usage.output_tokens,
                total_tokens: parsed.usage.input_tokens + parsed.usage.output_tokens,
            },
            finish_reason: parsed.stop_reason,
            tool_calls: None,
        })
    }

    pub async fn complete_streaming(&self, request: LlmRequest) -> Result<StreamingChunk> {
        self.complete(request).await?;
        Ok(StreamingChunk { delta: String::new(), done: true, usage: None })
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    model: String,
    content: Vec<serde_json::Value>,
    #[serde(rename = "stop_reason")]
    stop_reason: String,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    #[serde(rename = "input_tokens")]
    input_tokens: usize,
    #[serde(rename = "output_tokens")]
    output_tokens: usize,
}
