//! # Acode Core Traits
//!
//! 参考 hermes-agent-rs 的 trait 抽象层，定义所有可插拔组件的接口。
//!
//! 6 大核心 trait:
//! - [`LlmProvider`] — LLM API 调用（OpenAI / Anthropic / OpenRouter / Generic）
//! - [`ToolHandler`] — 工具执行（文件系统 / 终端 / 浏览器 / MCP / …）
//! - [`MemoryProvider`] — 持久化记忆（SQLite / Mem0 / Holographic / …）
//! - [`TerminalBackend`] — 命令执行环境（Local / Docker / SSH / Daytona / Modal / Singularity）
//! - [`SkillProvider`] — 技能管理（本地文件 + Hub）
//! - [`PlatformAdapter`] — 消息平台（Telegram / Discord / Slack / 微信 / …）

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

// ──────────────────────────────────────────────────────────────────────────
// LlmProvider
// ──────────────────────────────────────────────────────────────────────────

/// LLM 请求参数
#[derive(Debug, Clone)]
pub struct LlmCallRequest {
    /// 模型 ID（如 "gpt-4o", "claude-3-5-sonnet"）
    pub model: String,
    /// 对话消息
    pub messages: Vec<crate::llm::Message>,
    /// 可选工具列表（JSON Schema）
    pub tools: Option<Vec<LlmToolSchema>>,
    /// 最大输出 token 数
    pub max_tokens: Option<u32>,
    /// 采样温度（0-2）
    pub temperature: Option<f64>,
    /// 额外请求体字段
    pub extra_body: Option<Value>,
    /// 是否流式
    pub stream: bool,
}

/// LLM 工具的 JSON Schema 描述
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmToolSchema {
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(rename = "function")]
    pub function: LlmFunctionSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmFunctionSchema {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// LLM 响应
#[derive(Debug, Clone)]
pub struct LlmCallResponse {
    /// 文本内容
    pub content: String,
    /// 实际使用的模型
    pub model: String,
    /// Token 用量
    pub usage: LlmUsage,
    /// 完成原因（stop / length / tool_calls）
    pub finish_reason: String,
    /// 模型产生的工具调用
    pub tool_calls: Option<Vec<LlmToolCall>>,
}

/// LLM 工具调用
#[derive(Debug, Clone)]
pub struct LlmToolCall {
    pub id: String,
    pub name: String,
    /// JSON 参数字符串
    pub arguments: String,
}

/// Token 用量
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub total_tokens: usize,
}

/// 流式 chunk
#[derive(Debug, Clone)]
pub struct LlmStreamChunk {
    /// delta 内容
    pub delta: String,
    /// 是否是最后一个 chunk
    pub done: bool,
    /// 最终 usage（仅在 done=true 时有值）
    pub usage: Option<LlmUsage>,
    /// 推理内容（Anthropic 等支持）
    pub reasoning: Option<String>,
}

/// LLM Provider trait — 所有 LLM 后端必须实现此 trait
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 单次 chat completion
    async fn complete(&self, req: LlmCallRequest) -> Result<LlmCallResponse>;

    /// 流式 chat completion，返回一个 Stream
    fn complete_streaming(
        &self,
        req: LlmCallRequest,
    ) -> impl Stream<Item = Result<LlmStreamChunk>> + Send + '_;

    /// 返回该 provider 支持的模型列表
    fn supported_models(&self) -> Vec<String>;

    /// 返回 provider 名称（如 "openai", "anthropic"）
    fn name(&self) -> &str;

    /// 返回最大上下文长度
    fn max_context_length(&self, model: &str) -> usize;
}

// ──────────────────────────────────────────────────────────────────────────
// ToolHandler
// ──────────────────────────────────────────────────────────────────────────

/// 工具执行结果
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// 成功时为 Ok(content)，失败时为 Err(message)
    pub content: Result<String>,
    /// 执行耗时（毫秒）
    pub elapsed_ms: u64,
    /// 是否需要审批
    pub requires_approval: bool,
}

/// 工具处理 trait — 所有工具后端必须实现此 trait
#[async_trait]
pub trait ToolHandler: Send + Sync {
    /// 执行工具
    async fn execute(&self, params: Value) -> ToolOutput;

    /// 返回工具的 JSON Schema 描述
    fn schema(&self) -> LlmToolSchema;

    /// 工具名称
    fn name(&self) -> &str;

    /// 工具是否健康（健康检查）
    async fn health_check(&self) -> bool {
        true
    }
}

// ──────────────────────────────────────────────────────────────────────────
// MemoryProvider
// ──────────────────────────────────────────────────────────────────────────

/// 记忆条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub namespace: String,
    pub key: String,
    pub value: String,
    pub score: Option<f64>,
    pub created_at: Option<String>,
    pub accessed_at: Option<String>,
    pub access_count: Option<u32>,
}

/// 记忆 Provider trait — 所有记忆后端必须实现此 trait
#[async_trait]
pub trait MemoryProvider: Send + Sync {
    /// 存储记忆
    async fn save(&self, entry: &MemoryEntry) -> Result<()>;

    /// 检索记忆（按 namespace + key 精确匹配）
    async fn load(&self, namespace: &str, key: &str) -> Result<Option<MemoryEntry>>;

    /// 语义搜索（返回最相关的 top_k 条记忆）
    async fn search(
        &self,
        namespace: &str,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<MemoryEntry>>;

    /// 列出所有 namespace
    async fn list_namespaces(&self) -> Result<Vec<String>>;

    /// 删除记忆
    async fn delete(&self, namespace: &str, key: &str) -> Result<()>;

    /// 聚合统计
    async fn stats(&self) -> Result<MemoryStats>;
}

/// 记忆统计
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    pub total_entries: usize,
    pub namespaces: usize,
    pub total_size_bytes: usize,
}

// ──────────────────────────────────────────────────────────────────────────
// TerminalBackend
// ──────────────────────────────────────────────────────────────────────────

/// 命令执行结果
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub elapsed_ms: u64,
}

/// 终端执行环境 trait
#[async_trait]
pub trait TerminalBackend: Send + Sync {
    /// 执行命令
    async fn execute(
        &self,
        command: &str,
        timeout_secs: Option<u64>,
        workdir: Option<&str>,
    ) -> Result<CommandOutput>;

    /// 读文件
    async fn read_file(&self, path: &str) -> Result<String>;

    /// 写文件
    async fn write_file(&self, path: &str, content: &str) -> Result<()>;

    /// 检查路径是否存在
    async fn exists(&self, path: &str) -> bool;

    /// 环境类型（local / docker / ssh / …）
    fn backend_type(&self) -> &str;

    /// 健康检查
    async fn health_check(&self) -> bool {
        self.execute("echo ok", Some(5), None).await
            .is_ok()
    }
}

// ──────────────────────────────────────────────────────────────────────────
// SkillProvider
// ──────────────────────────────────────────────────────────────────────────

/// Skill 元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub path: String,
    pub success_rate: f64,
    pub utility_score: f64,
}

/// Skill 内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillContent {
    pub meta: SkillInfo,
    /// Skill 的 prompt / 指令
    pub instructions: String,
    /// 可选的工具列表
    pub tools: Vec<LlmToolSchema>,
}

/// Skill Provider trait
#[async_trait]
pub trait SkillProvider: Send + Sync {
    async fn get(&self, id: &str) -> Result<Option<SkillContent>>;
    async fn list(&self) -> Result<Vec<SkillInfo>>;
    async fn register(&self, content: SkillContent) -> Result<()>;
    async fn evolve(&self, id: &str, improved: &str) -> Result<()>;
    async fn health_check(&self) -> bool {
        self.list().await.is_ok()
    }
}

// ──────────────────────────────────────────────────────────────────────────
// PlatformAdapter
// ──────────────────────────────────────────────────────────────────────────

/// 平台消息
#[derive(Debug, Clone)]
pub struct PlatformMessage {
    pub chat_id: String,
    pub sender: String,
    pub text: String,
    pub timestamp: i64,
}

/// 平台适配器 trait
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// 启动监听
    async fn start(&self) -> Result<()>;
    /// 停止
    async fn stop(&self) -> Result<()>;
    /// 发送消息
    async fn send(&self, chat_id: &str, text: &str) -> Result<()>;
    /// 平台名称
    fn platform_name(&self) -> &str;
    /// 是否在运行
    fn is_running(&self) -> bool;
}
